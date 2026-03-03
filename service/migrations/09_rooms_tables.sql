-- Rooms module tables for the room and polling engine.
-- Rooms contain polls with multi-dimensional voting.

-- Rooms are containers for polls with eligibility rules.
CREATE TABLE IF NOT EXISTS rooms__rooms (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT,
    eligibility_topic TEXT NOT NULL DEFAULT 'identity_verified',
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed', 'archived')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at TIMESTAMPTZ,
    CONSTRAINT uq_rooms_name UNIQUE (name)
);

-- Polls belong to a room and have a lifecycle.
CREATE TABLE IF NOT EXISTS rooms__polls (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id UUID NOT NULL REFERENCES rooms__rooms(id) ON DELETE CASCADE,
    question TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'active', 'closed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    activated_at TIMESTAMPTZ,
    closed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_polls_room ON rooms__polls(room_id);

-- Dimensions define the axes of a multi-dimensional poll.
CREATE TABLE IF NOT EXISTS rooms__poll_dimensions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    poll_id UUID NOT NULL REFERENCES rooms__polls(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    min_value REAL NOT NULL DEFAULT 0.0,
    max_value REAL NOT NULL DEFAULT 1.0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT uq_poll_dimensions_poll_name UNIQUE (poll_id, name)
);

CREATE INDEX IF NOT EXISTS idx_poll_dimensions_poll ON rooms__poll_dimensions(poll_id);

-- Votes: one per user per dimension. Upsert to allow changing vote while poll is active.
CREATE TABLE IF NOT EXISTS rooms__votes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    poll_id UUID NOT NULL REFERENCES rooms__polls(id) ON DELETE CASCADE,
    dimension_id UUID NOT NULL REFERENCES rooms__poll_dimensions(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    value REAL NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_votes_poll_dimension_user UNIQUE (poll_id, dimension_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_votes_poll ON rooms__votes(poll_id);
CREATE INDEX IF NOT EXISTS idx_votes_user ON rooms__votes(user_id);
