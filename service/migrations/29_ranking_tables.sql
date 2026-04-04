-- Migration 29: Ranking tables for the meme ranking engine.
--
-- Adds tables for daily tournament rounds, submissions, pairwise matchups,
-- Glicko-2 ratings, and a hall of fame for daily winners.

-- Enums
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'ranking_round_status') THEN
        CREATE TYPE ranking_round_status AS ENUM ('submitting', 'ranking', 'closed');
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'submission_content_type') THEN
        CREATE TYPE submission_content_type AS ENUM ('url', 'image');
    END IF;
END $$;

-- One row per daily tournament cycle.
CREATE TABLE IF NOT EXISTS rooms__rounds (
    id              UUID                 PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id         UUID                 NOT NULL REFERENCES rooms__rooms(id) ON DELETE CASCADE,
    round_number    INT                  NOT NULL,
    submit_opens_at TIMESTAMPTZ          NOT NULL,
    rank_opens_at   TIMESTAMPTZ          NOT NULL,
    closes_at       TIMESTAMPTZ          NOT NULL,
    status          ranking_round_status NOT NULL DEFAULT 'submitting',
    created_at      TIMESTAMPTZ          NOT NULL DEFAULT now(),
    CONSTRAINT uq_rounds_room_number UNIQUE (room_id, round_number)
);

CREATE INDEX IF NOT EXISTS idx_rounds_room_status ON rooms__rounds (room_id, status);

-- One meme submission per user per round.
CREATE TABLE IF NOT EXISTS rooms__submissions (
    id           UUID                   PRIMARY KEY DEFAULT gen_random_uuid(),
    round_id     UUID                   NOT NULL REFERENCES rooms__rounds(id) ON DELETE CASCADE,
    author_id    UUID                   NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    content_type submission_content_type NOT NULL,
    url          TEXT,
    image_key    TEXT,
    caption      TEXT,
    created_at   TIMESTAMPTZ            NOT NULL DEFAULT now(),
    CONSTRAINT uq_submissions_round_author UNIQUE (round_id, author_id),
    CONSTRAINT chk_submissions_url_requires_url
        CHECK (content_type <> 'url' OR url IS NOT NULL),
    CONSTRAINT chk_submissions_image_requires_key
        CHECK (content_type <> 'image' OR image_key IS NOT NULL)
);

-- Every pairwise comparison a ranker makes within a round.
CREATE TABLE IF NOT EXISTS rooms__matchups (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    round_id     UUID        NOT NULL REFERENCES rooms__rounds(id) ON DELETE CASCADE,
    ranker_id    UUID        NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    submission_a UUID        NOT NULL REFERENCES rooms__submissions(id) ON DELETE CASCADE,
    submission_b UUID        NOT NULL REFERENCES rooms__submissions(id) ON DELETE CASCADE,
    winner_id    UUID        REFERENCES rooms__submissions(id) ON DELETE SET NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_matchups_ordered_pair CHECK (submission_a < submission_b),
    CONSTRAINT uq_matchups_round_ranker_pair UNIQUE (round_id, ranker_id, submission_a, submission_b)
);

CREATE INDEX IF NOT EXISTS idx_matchups_round         ON rooms__matchups (round_id);
CREATE INDEX IF NOT EXISTS idx_matchups_round_ranker  ON rooms__matchups (round_id, ranker_id);

-- Glicko-2 rating state per submission, updated after each round closes.
CREATE TABLE IF NOT EXISTS rooms__ratings (
    submission_id UUID            PRIMARY KEY REFERENCES rooms__submissions(id) ON DELETE CASCADE,
    rating        DOUBLE PRECISION NOT NULL DEFAULT 1500.0,
    deviation     DOUBLE PRECISION NOT NULL DEFAULT 350.0,
    volatility    DOUBLE PRECISION NOT NULL DEFAULT 0.06,
    matchup_count INT              NOT NULL DEFAULT 0
);

-- Daily winners archive.
CREATE TABLE IF NOT EXISTS rooms__hall_of_fame (
    id            UUID            PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id       UUID            NOT NULL REFERENCES rooms__rooms(id) ON DELETE CASCADE,
    round_id      UUID            NOT NULL REFERENCES rooms__rounds(id) ON DELETE CASCADE,
    submission_id UUID            NOT NULL REFERENCES rooms__submissions(id) ON DELETE CASCADE,
    final_rating  DOUBLE PRECISION NOT NULL,
    rank          INT              NOT NULL,
    created_at    TIMESTAMPTZ      NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_hall_of_fame_room_created
    ON rooms__hall_of_fame (room_id, created_at DESC);

-- pgmq queue for ranking lifecycle events (round transitions, rating jobs).
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pgmq.meta WHERE queue_name = 'rooms__ranking_lifecycle') THEN
        PERFORM pgmq.create('rooms__ranking_lifecycle');
    END IF;
END $$;
