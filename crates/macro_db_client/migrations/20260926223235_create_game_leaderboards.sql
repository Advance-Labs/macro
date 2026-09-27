-- Team leaderboards for the mini games. Game state itself lives in each game
-- room document on the sync service; these tables only rank results.
CREATE TYPE game_kind AS ENUM (
    'snake',
    'twenty_forty_eight',
    'minesweeper',
    'tic_tac_toe',
    'connect_four',
    'dots_and_boxes',
    'typing_race'
);

-- Each player's best result in a high-score game. A team leaderboard ranks
-- the rows of the team's current members, so scores follow their player and
-- disappear with the account.
CREATE TABLE game_best_score (
    user_id TEXT NOT NULL REFERENCES "User"(id) ON DELETE CASCADE,
    game_kind game_kind NOT NULL,
    score BIGINT NOT NULL,
    achieved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, game_kind)
);

-- One row per finished round of a two-player or party game room. Every
-- player's client reports the same round, so `(document_id, round)` keeps the
-- first report only. Deleting a room keeps its history (the reference is
-- cleared); deleting a winner's account removes their wins.
CREATE TABLE game_round_result (
    id UUID PRIMARY KEY,
    document_id TEXT REFERENCES "Document"(id) ON DELETE SET NULL,
    round INTEGER NOT NULL CHECK (round >= 0),
    game_kind game_kind NOT NULL,
    winner_user_id TEXT REFERENCES "User"(id) ON DELETE CASCADE,
    finished_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (document_id, round)
);

CREATE INDEX game_round_result_winner_idx
    ON game_round_result (winner_user_id, game_kind)
    WHERE winner_user_id IS NOT NULL;
