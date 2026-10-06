use std::ops::Deref;

use macro_db_migrator::MACRO_DB_MIGRATIONS;
use macro_user_id::cowlike::CowLike;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

use super::super::test::{CALL1, USER_A, USER_B, USER_C, give_user_a_team, repo};
use crate::domain::ports::CallRepository;
use crate::domain::recording::{
    CallKinds, CallKindsPatch, MeetingAttendance, MeetingKindChange, RecordingRules,
};

const TEAM_ID: Uuid = Uuid::from_u128(0x7ea3_0000_0000_0000_0000_0000_0000_00c1);

async fn user_a_on_a_team(pool: &Pool<Postgres>) -> anyhow::Result<()> {
    give_user_a_team(pool, USER_A.as_ref(), &TEAM_ID).await
}

#[sqlx::test(
    fixtures(path = "../../../../fixtures", scripts("call_repo")),
    migrator = "MACRO_DB_MIGRATIONS"
)]
async fn missing_rows_record_everything_and_block_nothing(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    user_a_on_a_team(&pool).await?;
    let repo = repo(pool);
    for team in [None, Some(TEAM_ID), Some(Uuid::now_v7())] {
        assert_eq!(
            repo.get_recording_rules(USER_A.deref().copied(), team)
                .await?,
            RecordingRules::default()
        );
    }
    Ok(())
}

#[sqlx::test(
    fixtures(path = "../../../../fixtures", scripts("call_repo")),
    migrator = "MACRO_DB_MIGRATIONS"
)]
async fn default_patches_merge_one_kind_at_a_time(pool: Pool<Postgres>) -> anyhow::Result<()> {
    user_a_on_a_team(&pool).await?;
    let repo = repo(pool);
    let huddles_off = repo
        .update_recording_defaults(
            USER_A.deref().copied(),
            CallKindsPatch {
                huddles: Some(false),
                ..CallKindsPatch::default()
            },
        )
        .await?;
    assert_eq!(
        huddles_off,
        CallKinds {
            huddles: false,
            ..CallKinds::ALL
        }
    );
    // A patch for another kind keeps the earlier one.
    let both_off = repo
        .update_recording_defaults(
            USER_A.deref().copied(),
            CallKindsPatch {
                external_meetings: Some(false),
                ..CallKindsPatch::default()
            },
        )
        .await?;
    assert_eq!(
        both_off,
        CallKinds {
            huddles: false,
            external_meetings: false,
            ..CallKinds::ALL
        }
    );
    let rules = repo
        .get_recording_rules(USER_A.deref().copied(), None)
        .await?;
    assert_eq!(rules.record_by_default, both_off);
    assert_eq!(rules.blocked, CallKinds::NONE);
    Ok(())
}

#[sqlx::test(
    fixtures(path = "../../../../fixtures", scripts("call_repo")),
    migrator = "MACRO_DB_MIGRATIONS"
)]
async fn team_blocks_apply_only_to_their_team(pool: Pool<Postgres>) -> anyhow::Result<()> {
    user_a_on_a_team(&pool).await?;
    let repo = repo(pool);
    let blocked = repo
        .update_team_recording_blocks(
            &TEAM_ID,
            CallKindsPatch {
                one_on_one_meetings: Some(true),
                internal_meetings: Some(true),
                ..CallKindsPatch::default()
            },
        )
        .await?;
    assert_eq!(
        blocked,
        CallKinds {
            one_on_one_meetings: true,
            internal_meetings: true,
            ..CallKinds::NONE
        }
    );
    let unblocked = repo
        .update_team_recording_blocks(
            &TEAM_ID,
            CallKindsPatch {
                internal_meetings: Some(false),
                huddles: Some(true),
                ..CallKindsPatch::default()
            },
        )
        .await?;
    assert_eq!(
        unblocked,
        CallKinds {
            huddles: true,
            one_on_one_meetings: true,
            ..CallKinds::NONE
        }
    );
    assert_eq!(
        repo.get_recording_rules(USER_A.deref().copied(), Some(TEAM_ID))
            .await?
            .blocked,
        unblocked
    );
    assert_eq!(
        repo.get_recording_rules(USER_A.deref().copied(), None)
            .await?
            .blocked,
        CallKinds::NONE
    );
    Ok(())
}

fn attendance(more_than_two: bool, external: bool) -> Option<MeetingAttendance> {
    Some(MeetingAttendance {
        more_than_two,
        external,
    })
}

#[sqlx::test(
    fixtures(path = "../../../../fixtures", scripts("call_repo")),
    migrator = "MACRO_DB_MIGRATIONS"
)]
async fn a_call_turns_external_once_and_reports_its_recorder(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = repo(pool);
    assert_eq!(
        repo.get_meeting_attendance(&CALL1).await?,
        attendance(false, false)
    );
    repo.set_egress_id(&CALL1, "egress-1").await?;

    assert_eq!(
        repo.mark_call_external(&CALL1).await?,
        Some(MeetingKindChange {
            egress_id: Some("egress-1".to_string()),
        })
    );
    assert_eq!(
        repo.get_meeting_attendance(&CALL1).await?,
        attendance(false, true)
    );
    assert_eq!(repo.mark_call_external(&CALL1).await?, None);

    // A call that is no longer live is neither flagged nor flaggable.
    let ended = Uuid::now_v7();
    assert_eq!(repo.mark_call_external(&ended).await?, None);
    assert_eq!(repo.get_meeting_attendance(&ended).await?, None);
    Ok(())
}

#[sqlx::test(
    fixtures(path = "../../../../fixtures", scripts("call_repo")),
    migrator = "MACRO_DB_MIGRATIONS"
)]
async fn a_third_person_ends_the_one_on_one_once(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = repo(pool);
    repo.set_egress_id(&CALL1, "egress-1").await?;
    // The fixture call has two participants.
    assert_eq!(repo.mark_call_more_than_two(&CALL1).await?, None);

    // Leaving and rejoining is still the same two people.
    repo.remove_participant(&CALL1, USER_B.deref().copied())
        .await?;
    repo.add_participant(&CALL1, USER_B.deref().copied())
        .await?;
    assert_eq!(repo.mark_call_more_than_two(&CALL1).await?, None);

    // A third person counts even after someone else has left.
    repo.remove_participant(&CALL1, USER_B.deref().copied())
        .await?;
    repo.add_participant(&CALL1, USER_C.deref().copied())
        .await?;
    assert_eq!(
        repo.mark_call_more_than_two(&CALL1).await?,
        Some(MeetingKindChange {
            egress_id: Some("egress-1".to_string()),
        })
    );
    assert_eq!(
        repo.get_meeting_attendance(&CALL1).await?,
        attendance(true, false)
    );
    assert_eq!(repo.mark_call_more_than_two(&CALL1).await?, None);

    // A later outsider still turns the call external.
    assert!(repo.mark_call_external(&CALL1).await?.is_some());
    assert_eq!(
        repo.get_meeting_attendance(&CALL1).await?,
        attendance(true, true)
    );
    Ok(())
}

#[sqlx::test(
    fixtures(path = "../../../../fixtures", scripts("call_repo")),
    migrator = "MACRO_DB_MIGRATIONS"
)]
async fn an_external_call_never_becomes_an_internal_one(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let repo = repo(pool);
    assert!(repo.mark_call_external(&CALL1).await?.is_some());
    repo.add_participant(&CALL1, USER_C.deref().copied())
        .await?;
    assert_eq!(repo.mark_call_more_than_two(&CALL1).await?, None);
    assert_eq!(
        repo.get_meeting_attendance(&CALL1).await?,
        attendance(false, true)
    );
    Ok(())
}

#[sqlx::test(
    fixtures(path = "../../../../fixtures", scripts("call_repo")),
    migrator = "MACRO_DB_MIGRATIONS"
)]
async fn a_call_claims_its_recorder_once(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let repo = repo(pool);
    assert!(repo.claim_meeting_recorder(&CALL1).await?);
    assert!(!repo.claim_meeting_recorder(&CALL1).await?);
    assert!(!repo.claim_meeting_recorder(&Uuid::now_v7()).await?);
    Ok(())
}
