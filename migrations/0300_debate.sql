-- Debate: threaded comments with edit history, reports, moderation history,
-- thread locks and amendments (suggestions that never change the published text).

-- `body` holds the current text; every version (including the first) is kept in
-- `comment_revisions`. Moderation hides a comment from the public view
-- (`status = 'removed'`) without deleting its content.
CREATE TABLE comments (
    id TEXT PRIMARY KEY,
    proposal_id TEXT NOT NULL REFERENCES proposals(id),
    author_id TEXT NOT NULL REFERENCES users(id),
    parent_id TEXT REFERENCES comments(id),
    -- Top-level comment of the thread (its own id at depth 0).
    root_id TEXT NOT NULL,
    depth INTEGER NOT NULL CHECK (depth BETWEEN 0 AND 2),
    body TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('visible', 'removed')),
    created_at TEXT NOT NULL,
    edited_at TEXT,
    removed_at TEXT,
    CHECK ((parent_id IS NULL) = (depth = 0)),
    CHECK ((depth = 0) = (root_id = id)),
    CHECK ((status = 'removed') = (removed_at IS NOT NULL))
);
CREATE INDEX comments_thread ON comments(proposal_id, parent_id, created_at);
CREATE INDEX comments_root ON comments(root_id, created_at);
CREATE INDEX comments_author_recent ON comments(author_id, created_at);
CREATE TRIGGER comments_identity_immutable BEFORE UPDATE OF
    id, proposal_id, author_id, parent_id, root_id, depth, created_at
ON comments
BEGIN
    SELECT RAISE(ABORT, 'comment identity is immutable');
END;
CREATE TRIGGER comments_no_delete BEFORE DELETE ON comments
BEGIN
    SELECT RAISE(ABORT, 'comments are preserved');
END;

-- Append-only edit history: revision 1 is the original text.
CREATE TABLE comment_revisions (
    comment_id TEXT NOT NULL REFERENCES comments(id),
    rev INTEGER NOT NULL CHECK (rev >= 1),
    body TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (comment_id, rev)
);
CREATE INDEX comment_revisions_created ON comment_revisions(created_at);
CREATE TRIGGER comment_revisions_no_update BEFORE UPDATE ON comment_revisions
BEGIN
    SELECT RAISE(ABORT, 'comment history is append-only');
END;
CREATE TRIGGER comment_revisions_no_delete BEFORE DELETE ON comment_revisions
BEGIN
    SELECT RAISE(ABORT, 'comment history is append-only');
END;

-- One report per account and comment; resolved by moderation.
CREATE TABLE comment_reports (
    id TEXT PRIMARY KEY,
    comment_id TEXT NOT NULL REFERENCES comments(id),
    reporter_id TEXT NOT NULL REFERENCES users(id),
    reason TEXT NOT NULL CHECK (reason IN ('spam', 'offensive', 'off_topic', 'misinformation', 'other')),
    details TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('open', 'dismissed', 'actioned')),
    created_at TEXT NOT NULL,
    resolved_at TEXT,
    resolved_by TEXT REFERENCES users(id),
    CHECK ((status = 'open') = (resolved_at IS NULL)),
    UNIQUE (comment_id, reporter_id)
);
CREATE INDEX comment_reports_open ON comment_reports(status, created_at);
CREATE TRIGGER comment_reports_no_delete BEFORE DELETE ON comment_reports
BEGIN
    SELECT RAISE(ABORT, 'reports are preserved');
END;

-- Append-only moderation history (the audit chain records the same actions).
CREATE TABLE moderation_actions (
    id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL REFERENCES users(id),
    proposal_id TEXT NOT NULL REFERENCES proposals(id),
    target_kind TEXT NOT NULL CHECK (target_kind IN ('comment', 'thread')),
    target_id TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('remove', 'restore', 'lock', 'unlock', 'dismiss_report')),
    reason TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX moderation_actions_target ON moderation_actions(target_id, created_at);
CREATE TRIGGER moderation_actions_no_update BEFORE UPDATE ON moderation_actions
BEGIN
    SELECT RAISE(ABORT, 'moderation history is append-only');
END;
CREATE TRIGGER moderation_actions_no_delete BEFORE DELETE ON moderation_actions
BEGIN
    SELECT RAISE(ABORT, 'moderation history is append-only');
END;

-- Current lock state per proposal debate (history in moderation_actions).
CREATE TABLE debate_locks (
    proposal_id TEXT PRIMARY KEY REFERENCES proposals(id),
    locked INTEGER NOT NULL CHECK (locked IN (0, 1)),
    reason TEXT NOT NULL,
    changed_by TEXT NOT NULL REFERENCES users(id),
    changed_at TEXT NOT NULL
);

-- Suggested changes bound to the hash of the published text they refer to.
CREATE TABLE amendments (
    id TEXT PRIMARY KEY,
    proposal_id TEXT NOT NULL REFERENCES proposals(id),
    author_id TEXT NOT NULL REFERENCES users(id),
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    text_hash TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('open', 'withdrawn', 'addressed')),
    status_note TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK ((status = 'addressed') = (status_note IS NOT NULL))
);
CREATE INDEX amendments_proposal ON amendments(proposal_id, created_at);
CREATE INDEX amendments_author_recent ON amendments(author_id, created_at);
CREATE TRIGGER amendments_content_immutable BEFORE UPDATE OF
    id, proposal_id, author_id, title, body, text_hash, created_at
ON amendments
BEGIN
    SELECT RAISE(ABORT, 'amendment content is immutable');
END;
CREATE TRIGGER amendments_status_final BEFORE UPDATE ON amendments
WHEN OLD.status <> 'open'
BEGIN
    SELECT RAISE(ABORT, 'amendment status is final');
END;
CREATE TRIGGER amendments_no_delete BEFORE DELETE ON amendments
BEGIN
    SELECT RAISE(ABORT, 'amendments are preserved');
END;
