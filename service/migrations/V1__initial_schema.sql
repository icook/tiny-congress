-- Create extension for UUID generation
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Topics table
CREATE TABLE topics (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Rounds table
CREATE TABLE rounds (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Pairings table
CREATE TABLE pairings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    round_id UUID NOT NULL REFERENCES rounds(id),
    topic_a_id UUID NOT NULL REFERENCES topics(id),
    topic_b_id UUID NOT NULL REFERENCES topics(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT different_topics CHECK (topic_a_id <> topic_b_id)
);

-- Votes table
CREATE TABLE votes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pairing_id UUID NOT NULL REFERENCES pairings(id),
    user_id TEXT NOT NULL,
    choice_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add vote constraint (using trigger instead of CHECK constraint for better compatibility)
CREATE OR REPLACE FUNCTION validate_vote_choice()
RETURNS TRIGGER AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pairings 
        WHERE id = NEW.pairing_id 
        AND (topic_a_id = NEW.choice_id OR topic_b_id = NEW.choice_id)
    ) THEN
        RAISE EXCEPTION 'Invalid choice for pairing';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER validate_vote_choice_trigger
BEFORE INSERT ON votes
FOR EACH ROW
EXECUTE FUNCTION validate_vote_choice();

-- Topic rankings table
CREATE TABLE topic_rankings (
    topic_id UUID PRIMARY KEY REFERENCES topics(id),
    rank INTEGER NOT NULL,
    score FLOAT NOT NULL DEFAULT 1500.0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes
CREATE INDEX idx_pairings_round_id ON pairings(round_id);
CREATE INDEX idx_votes_pairing_id ON votes(pairing_id);
CREATE INDEX idx_votes_user_id ON votes(user_id);
CREATE INDEX idx_topic_rankings_score ON topic_rankings(score DESC);

-- Try to create PGMQ extension and queue if available
DO $$
BEGIN
    BEGIN
        -- Check if pgmq extension exists
        EXECUTE 'CREATE EXTENSION IF NOT EXISTS pgmq';
        
        -- Create the queue if the extension was loaded successfully
        EXECUTE 'SELECT pgmq.create(''vote_queue'')';
    EXCEPTION WHEN OTHERS THEN
        RAISE NOTICE 'PGMQ extension not available. Skipping queue creation.';
    END;
END;
$$;