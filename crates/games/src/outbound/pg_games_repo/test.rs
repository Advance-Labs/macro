use macro_db_migrator::MACRO_DB_MIGRATIONS;
use macro_user_id::user_id::MacroUserIdStr;
use sqlx::PgPool;
use uuid::Uuid;

use super::PgGamesRepo;
use crate::domain::models::{GameKind, LeaderboardScope, NewRoundResult, RoundWrite};
use crate::domain::ports::GamesRepo;

const ANN: &str = "macro|games-ann@macro.com";
const BOB: &str = "macro|games-bob@macro.com";
const CAT: &str = "macro|games-cat@macro.com";

fn user(id: &str) -> MacroUserIdStr<'static> {
    MacroUserIdStr::try_from(id.to_owned()).expect("valid user id")
}

async fn insert_user(pool: &PgPool, id: &str) {
    let macro_user_id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO macro_user (id, username, email, stripe_customer_id) VALUES ($1, $2, $2, $2)"#,
    )
    .bind(macro_user_id)
    .bind(id)
    .execute(pool)
    .await
    .expect("macro_user should insert");
    sqlx::query(r#"INSERT INTO "User" (id, email, macro_user_id) VALUES ($1, $1, $2)"#)
        .bind(id)
        .bind(macro_user_id)
        .execute(pool)
        .await
        .expect("user should insert");
}

async fn insert_team(pool: &PgPool, members: &[&str]) -> Uuid {
    let team_id = Uuid::now_v7();
    sqlx::query(r#"INSERT INTO team (id, name, owner_id) VALUES ($1, 'Arcade', $2)"#)
        .bind(team_id)
        .bind(members[0])
        .execute(pool)
        .await
        .expect("team should insert");
    for (index, member) in members.iter().enumerate() {
        let role = if index == 0 { "owner" } else { "member" };
        sqlx::query(
            r#"INSERT INTO team_user (user_id, team_id, team_role) VALUES ($1, $2, $3::team_role)"#,
        )
        .bind(member)
        .bind(team_id)
        .bind(role)
        .execute(pool)
        .await
        .expect("team_user should insert");
    }
    team_id
}

async fn insert_room(pool: &PgPool, owner: &str) -> String {
    let id = Uuid::now_v7().to_string();
    sqlx::query(r#"INSERT INTO "Document" (id, name, owner, "fileType") VALUES ($1, 'Connect Four', $2, 'game')"#)
        .bind(&id)
        .bind(owner)
        .execute(pool)
        .await
        .expect("document should insert");
    id
}

async fn seed_players(pool: &PgPool) -> Uuid {
    for id in [ANN, BOB, CAT] {
        insert_user(pool, id).await;
    }
    // Cat has no team.
    insert_team(pool, &[ANN, BOB]).await
}

fn round(document_id: &str, round: i32, winner: Option<&str>) -> NewRoundResult {
    NewRoundResult {
        id: Uuid::now_v7(),
        document_id: document_id.to_string(),
        round,
        kind: GameKind::ConnectFour,
        winner: winner.map(user),
    }
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn keeps_the_higher_score_for_high_score_games(pool: PgPool) {
    seed_players(&pool).await;
    let repo = PgGamesRepo::new(pool);

    let (first, improved) = repo
        .record_best_score(&user(ANN), GameKind::Snake, 120)
        .await
        .unwrap();
    assert_eq!((first.score, improved), (120, true));
    let (worse, improved) = repo
        .record_best_score(&user(ANN), GameKind::Snake, 80)
        .await
        .unwrap();
    assert_eq!((worse.score, improved), (120, false));
    assert_eq!(worse.achieved_at, first.achieved_at);
    let (better, improved) = repo
        .record_best_score(&user(ANN), GameKind::Snake, 300)
        .await
        .unwrap();
    assert_eq!((better.score, improved), (300, true));
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn keeps_the_lower_score_for_timed_games(pool: PgPool) {
    seed_players(&pool).await;
    let repo = PgGamesRepo::new(pool);

    repo.record_best_score(&user(ANN), GameKind::Minesweeper, 40_000)
        .await
        .unwrap();
    let (slower, improved) = repo
        .record_best_score(&user(ANN), GameKind::Minesweeper, 55_000)
        .await
        .unwrap();
    assert_eq!((slower.score, improved), (40_000, false));
    let (faster, improved) = repo
        .record_best_score(&user(ANN), GameKind::Minesweeper, 31_500)
        .await
        .unwrap();
    assert_eq!((faster.score, improved), (31_500, true));
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn team_scope_ranks_only_current_members(pool: PgPool) {
    let team_id = seed_players(&pool).await;
    let repo = PgGamesRepo::new(pool.clone());
    for (id, score) in [(ANN, 100), (BOB, 250), (CAT, 900)] {
        repo.record_best_score(&user(id), GameKind::TwentyFortyEight, score)
            .await
            .unwrap();
    }

    let mut team = repo
        .best_scores(&LeaderboardScope::Team(team_id))
        .await
        .unwrap();
    team.sort_by_key(|best| best.score);
    assert_eq!(
        team.iter()
            .map(|best| (best.user_id.to_string(), best.score))
            .collect::<Vec<_>>(),
        vec![(ANN.to_string(), 100), (BOB.to_string(), 250)]
    );

    let solo = repo
        .best_scores(&LeaderboardScope::User(user(CAT)))
        .await
        .unwrap();
    assert_eq!(solo.len(), 1);
    assert_eq!(solo[0].score, 900);

    // Leaving the team removes the player from its leaderboard.
    sqlx::query("DELETE FROM team_user WHERE user_id = $1")
        .bind(BOB)
        .execute(&pool)
        .await
        .unwrap();
    let after = repo
        .best_scores(&LeaderboardScope::Team(team_id))
        .await
        .unwrap();
    assert_eq!(after.len(), 1);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn records_each_round_once_and_tallies_outright_wins(pool: PgPool) {
    let team_id = seed_players(&pool).await;
    let room = insert_room(&pool, ANN).await;
    let repo = PgGamesRepo::new(pool.clone());

    assert_eq!(
        repo.record_round(&round(&room, 0, Some(ANN)))
            .await
            .unwrap(),
        RoundWrite::Recorded
    );
    // The opponent's client reports the same round.
    assert_eq!(
        repo.record_round(&round(&room, 0, Some(ANN)))
            .await
            .unwrap(),
        RoundWrite::AlreadyRecorded
    );
    repo.record_round(&round(&room, 1, Some(BOB)))
        .await
        .unwrap();
    repo.record_round(&round(&room, 2, None)).await.unwrap();
    repo.record_round(&round(&room, 3, Some(ANN)))
        .await
        .unwrap();

    let mut tallies = repo
        .win_tallies(&LeaderboardScope::Team(team_id))
        .await
        .unwrap();
    tallies.sort_by_key(|tally| tally.wins);
    assert_eq!(
        tallies
            .iter()
            .map(|tally| (tally.user_id.to_string(), tally.kind, tally.wins))
            .collect::<Vec<_>>(),
        vec![
            (BOB.to_string(), GameKind::ConnectFour, 1),
            (ANN.to_string(), GameKind::ConnectFour, 2),
        ]
    );

    // Deleting the room keeps its history.
    sqlx::query(r#"DELETE FROM "Document" WHERE id = $1"#)
        .bind(&room)
        .execute(&pool)
        .await
        .unwrap();
    let solo = repo
        .win_tallies(&LeaderboardScope::User(user(ANN)))
        .await
        .unwrap();
    assert_eq!(solo[0].wins, 2);
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn rejects_rounds_for_unknown_rooms_or_players(pool: PgPool) {
    seed_players(&pool).await;
    let room = insert_room(&pool, ANN).await;
    let repo = PgGamesRepo::new(pool);

    assert_eq!(
        repo.record_round(&round("missing-room", 0, Some(ANN)))
            .await
            .unwrap(),
        RoundWrite::UnknownReference
    );
    assert_eq!(
        repo.record_round(&round(&room, 0, Some("macro|nobody@macro.com")))
            .await
            .unwrap(),
        RoundWrite::UnknownReference
    );
}
