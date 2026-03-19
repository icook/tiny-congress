-- Room owner: the account that created the room. Used for capability tier gates.
-- Existing rooms get NULL (no owner), new rooms require it.
ALTER TABLE rooms__rooms ADD COLUMN IF NOT EXISTS owner_id UUID REFERENCES accounts(id);

-- Per-room role assignments for elevated access (beyond base participant).
-- The endorsement gets users in the door; role assignment differentiates them.
CREATE TABLE IF NOT EXISTS rooms__role_assignments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id UUID NOT NULL REFERENCES rooms__rooms(id),
    account_id UUID NOT NULL REFERENCES accounts(id),
    role TEXT NOT NULL,          -- e.g. "contributor", "moderator"
    assigned_by UUID NOT NULL REFERENCES accounts(id),
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(room_id, account_id)  -- one role per user per room
);
