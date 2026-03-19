CREATE TABLE rooms__bot_traces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id UUID NOT NULL REFERENCES rooms__rooms(id),
    poll_id UUID REFERENCES rooms__polls(id),
    task TEXT NOT NULL,
    run_mode TEXT NOT NULL,
    steps JSONB NOT NULL DEFAULT '[]',
    total_cost_usd NUMERIC(10,6) NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'running',
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ
);

CREATE INDEX idx_bot_traces_room_id ON rooms__bot_traces(room_id);
CREATE INDEX idx_bot_traces_poll_id ON rooms__bot_traces(poll_id);
CREATE INDEX idx_bot_traces_status ON rooms__bot_traces(status);
