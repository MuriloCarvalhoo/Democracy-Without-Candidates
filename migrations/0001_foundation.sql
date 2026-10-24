-- Foundation: instance flags, territorial catalog, audit chain, notifications.
-- Conventions: ids TEXT (UUID), instants TEXT ISO-8601 UTC, booleans INTEGER 0/1.

CREATE TABLE feature_flags (
    key TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    updated_at TEXT NOT NULL,
    updated_by TEXT
);

CREATE TABLE synthetic_counter (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    value INTEGER NOT NULL CHECK (value >= 0),
    updated_at TEXT NOT NULL
);

CREATE TABLE catalog_versions (
    catalog TEXT PRIMARY KEY,
    checksum TEXT NOT NULL,
    imported_at TEXT NOT NULL
);

CREATE TABLE jurisdictions (
    id TEXT PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('national', 'state', 'municipal')),
    name TEXT NOT NULL,
    parent_id TEXT REFERENCES jurisdictions(id),
    state_abbrev TEXT,
    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1))
);
CREATE INDEX jurisdictions_parent ON jurisdictions(parent_id);
CREATE INDEX jurisdictions_name ON jurisdictions(kind, state_abbrev, name);

-- Append-only event log with a local hash chain. The chain detects accidental or
-- partial edits; it does not prove integrity against a privileged operator.
CREATE TABLE audit_events (
    seq INTEGER PRIMARY KEY,
    occurred_at TEXT NOT NULL,
    kind TEXT NOT NULL,
    visibility TEXT NOT NULL CHECK (visibility IN ('public', 'restricted')),
    actor_ref TEXT,
    subject TEXT,
    payload TEXT NOT NULL,
    prev_hash TEXT NOT NULL,
    hash TEXT NOT NULL UNIQUE
);
CREATE INDEX audit_events_kind ON audit_events(kind, seq);
CREATE TRIGGER audit_events_no_update BEFORE UPDATE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit_events is append-only');
END;
CREATE TRIGGER audit_events_no_delete BEFORE DELETE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit_events is append-only');
END;
