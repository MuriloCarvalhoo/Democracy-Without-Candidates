-- Audit checkpoints, public listing index, notification preferences and
-- scheduled (deadline) notifications.

CREATE INDEX audit_events_visibility ON audit_events(visibility, seq);

-- Local record of signed checkpoints. The exported JSON stored outside the VPS is
-- the evidence; this table only lists what was exported. Append-only.
CREATE TABLE audit_checkpoints (
    id TEXT PRIMARY KEY,
    seq INTEGER NOT NULL,
    head_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    public_key TEXT NOT NULL,
    signature TEXT NOT NULL
);
CREATE INDEX audit_checkpoints_created ON audit_checkpoints(created_at);
CREATE TRIGGER audit_checkpoints_no_update BEFORE UPDATE ON audit_checkpoints
BEGIN
    SELECT RAISE(ABORT, 'audit_checkpoints is append-only');
END;
CREATE TRIGGER audit_checkpoints_no_delete BEFORE DELETE ON audit_checkpoints
BEGIN
    SELECT RAISE(ABORT, 'audit_checkpoints is append-only');
END;

-- Absent row = enabled (default: all in-app categories on).
CREATE TABLE notification_preferences (
    user_id TEXT NOT NULL REFERENCES users(id),
    category TEXT NOT NULL CHECK (category IN ('proposal_created', 'voting_opened', 'deadline_reminder', 'result_published', 'phase_changed')),
    in_app INTEGER NOT NULL CHECK (in_app IN (0, 1)),
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_id, category)
);

CREATE INDEX notifications_user_created ON notifications(user_id, created_at);

-- Generic hook for reminders: features schedule a notice for one user or for every
-- active participant of a jurisdiction; `remind_due` delivers it once.
CREATE TABLE scheduled_notifications (
    id TEXT PRIMARY KEY,
    dedupe_key TEXT NOT NULL UNIQUE,
    user_id TEXT REFERENCES users(id),
    jurisdiction_id TEXT REFERENCES jurisdictions(id),
    event TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    link TEXT,
    due_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    delivered_at TEXT,
    CHECK ((user_id IS NULL) != (jurisdiction_id IS NULL))
);
CREATE INDEX scheduled_notifications_due ON scheduled_notifications(delivered_at, due_at);
