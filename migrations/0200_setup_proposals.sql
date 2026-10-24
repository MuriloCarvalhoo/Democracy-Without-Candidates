-- Setup versions, proposals, electorate snapshots, signatures and idempotency keys.
-- Percentages in basis points (10000 = 100%); durations in whole days.

-- One row per setup package. `proposed` packages await a plebiscite/meta-law;
-- `adopted` is effective for elections opened afterwards; the previous adopted
-- version of the same jurisdiction becomes `superseded`. `version` is assigned on
-- adoption and is unique per jurisdiction.
CREATE TABLE setup_versions (
    id TEXT PRIMARY KEY,
    jurisdiction_id TEXT NOT NULL REFERENCES jurisdictions(id),
    version INTEGER,
    status TEXT NOT NULL CHECK (status IN ('proposed', 'adopted', 'superseded', 'rejected')),
    synthetic INTEGER NOT NULL DEFAULT 0 CHECK (synthetic IN (0, 1)),
    source_proposal_id TEXT,
    signature_bp_national INTEGER NOT NULL CHECK (signature_bp_national BETWEEN 1 AND 10000),
    signature_bp_state INTEGER NOT NULL CHECK (signature_bp_state BETWEEN 1 AND 10000),
    signature_bp_municipal INTEGER NOT NULL CHECK (signature_bp_municipal BETWEEN 1 AND 10000),
    normal_quorum_bp INTEGER NOT NULL CHECK (normal_quorum_bp BETWEEN 1 AND 10000),
    normal_majority_bp INTEGER NOT NULL CHECK (normal_majority_bp BETWEEN 5000 AND 9999),
    normal_collect_days INTEGER NOT NULL CHECK (normal_collect_days BETWEEN 1 AND 365),
    normal_debate_days INTEGER NOT NULL CHECK (normal_debate_days BETWEEN 1 AND 180),
    normal_vote_days INTEGER NOT NULL CHECK (normal_vote_days BETWEEN 1 AND 90),
    urgent_quorum_bp INTEGER NOT NULL CHECK (urgent_quorum_bp BETWEEN 1 AND 10000),
    urgent_majority_bp INTEGER NOT NULL CHECK (urgent_majority_bp BETWEEN 5000 AND 9999),
    urgent_collect_days INTEGER NOT NULL CHECK (urgent_collect_days BETWEEN 1 AND 365),
    urgent_vote_days INTEGER NOT NULL CHECK (urgent_vote_days BETWEEN 1 AND 90),
    setup_quorum_bp INTEGER NOT NULL CHECK (setup_quorum_bp BETWEEN 1 AND 10000),
    setup_majority_bp INTEGER NOT NULL CHECK (setup_majority_bp BETWEEN 5000 AND 9999),
    setup_collect_days INTEGER NOT NULL CHECK (setup_collect_days BETWEEN 1 AND 365),
    setup_debate_days INTEGER NOT NULL CHECK (setup_debate_days BETWEEN 1 AND 180),
    setup_vote_days INTEGER NOT NULL CHECK (setup_vote_days BETWEEN 1 AND 90),
    initial_quorum_bp INTEGER NOT NULL CHECK (initial_quorum_bp BETWEEN 1 AND 10000),
    initial_majority_bp INTEGER NOT NULL CHECK (initial_majority_bp BETWEEN 5000 AND 9999),
    initial_vote_days INTEGER NOT NULL CHECK (initial_vote_days BETWEEN 1 AND 90),
    quota_normal_per_year INTEGER NOT NULL CHECK (quota_normal_per_year BETWEEN 1 AND 100),
    quota_urgent_per_year INTEGER NOT NULL CHECK (quota_urgent_per_year BETWEEN 1 AND 100),
    created_at TEXT NOT NULL,
    adopted_at TEXT,
    CHECK ((status = 'proposed' OR status = 'rejected') = (version IS NULL)),
    CHECK ((status IN ('adopted', 'superseded')) = (adopted_at IS NOT NULL)),
    UNIQUE (jurisdiction_id, version)
);
CREATE INDEX setup_versions_effective ON setup_versions(jurisdiction_id, status, version);

-- Parameters are immutable; only the lifecycle columns may change. (A column list
-- instead of a long OR chain keeps trigger compilation shallow in the engine.)
CREATE TRIGGER setup_versions_params_immutable BEFORE UPDATE OF
    jurisdiction_id, signature_bp_national, signature_bp_state, signature_bp_municipal,
    normal_quorum_bp, normal_majority_bp, normal_collect_days, normal_debate_days, normal_vote_days,
    urgent_quorum_bp, urgent_majority_bp, urgent_collect_days, urgent_vote_days,
    setup_quorum_bp, setup_majority_bp, setup_collect_days, setup_debate_days, setup_vote_days,
    initial_quorum_bp, initial_majority_bp, initial_vote_days, quota_normal_per_year, quota_urgent_per_year
ON setup_versions
BEGIN
    SELECT RAISE(ABORT, 'setup parameters are immutable');
END;
CREATE TRIGGER setup_versions_no_delete BEFORE DELETE ON setup_versions
BEGIN
    SELECT RAISE(ABORT, 'setup history is append-only');
END;

-- Eligible accounts materialized when a phase opens (collection or voting).
CREATE TABLE electorate_snapshots (
    id TEXT PRIMARY KEY,
    proposal_id TEXT NOT NULL,
    phase TEXT NOT NULL CHECK (phase IN ('collection', 'voting')),
    jurisdiction_id TEXT NOT NULL REFERENCES jurisdictions(id),
    setup_version_id TEXT NOT NULL REFERENCES setup_versions(id),
    member_count INTEGER NOT NULL CHECK (member_count >= 0),
    taken_at TEXT NOT NULL,
    UNIQUE (proposal_id, phase)
);
CREATE TABLE electorate_members (
    snapshot_id TEXT NOT NULL REFERENCES electorate_snapshots(id),
    user_id TEXT NOT NULL REFERENCES users(id),
    PRIMARY KEY (snapshot_id, user_id)
);
CREATE TRIGGER electorate_snapshots_immutable BEFORE UPDATE ON electorate_snapshots
BEGIN
    SELECT RAISE(ABORT, 'electorate snapshot is immutable');
END;
CREATE TRIGGER electorate_snapshots_no_delete BEFORE DELETE ON electorate_snapshots
BEGIN
    SELECT RAISE(ABORT, 'electorate snapshot is immutable');
END;
CREATE TRIGGER electorate_members_no_update BEFORE UPDATE ON electorate_members
BEGIN
    SELECT RAISE(ABORT, 'electorate snapshot is immutable');
END;
CREATE TRIGGER electorate_members_no_delete BEFORE DELETE ON electorate_members
BEGIN
    SELECT RAISE(ABORT, 'electorate snapshot is immutable');
END;

CREATE TABLE proposals (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('normal_law', 'urgent_law', 'setup_change', 'initial_setup')),
    jurisdiction_id TEXT NOT NULL REFERENCES jurisdictions(id),
    author_id TEXT NOT NULL REFERENCES users(id),
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    body TEXT NOT NULL,
    normalized_title TEXT NOT NULL,
    normalized_body_hash TEXT NOT NULL,
    text_hash TEXT,
    status TEXT NOT NULL CHECK (status IN ('draft', 'collecting_signatures', 'in_debate', 'in_voting',
        'approved', 'rejected', 'quorum_not_reached', 'expired', 'archived')),
    -- Package voted by setup_change/initial_setup proposals.
    proposed_setup_id TEXT REFERENCES setup_versions(id),
    -- Rules at collection opening (signature share, collection/debate durations).
    setup_version_id TEXT REFERENCES setup_versions(id),
    collection_snapshot_id TEXT REFERENCES electorate_snapshots(id),
    signature_target INTEGER CHECK (signature_target >= 1),
    signature_count INTEGER NOT NULL DEFAULT 0 CHECK (signature_count >= 0),
    quota_year INTEGER,
    collect_ends_at TEXT,
    debate_ends_at TEXT,
    -- Rules and electorate at voting opening (vote duration, quorum, majority).
    vote_setup_version_id TEXT REFERENCES setup_versions(id),
    vote_snapshot_id TEXT REFERENCES electorate_snapshots(id),
    vote_ends_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    published_at TEXT,
    debate_started_at TEXT,
    vote_started_at TEXT,
    closed_at TEXT,
    archived_at TEXT,
    archive_reason TEXT,
    CHECK ((status = 'draft') = (published_at IS NULL)),
    CHECK (status = 'draft' OR text_hash IS NOT NULL),
    CHECK ((status = 'archived') = (archived_at IS NOT NULL))
);
CREATE INDEX proposals_status ON proposals(status, published_at);
CREATE INDEX proposals_listing ON proposals(kind, status, jurisdiction_id);
CREATE INDEX proposals_author ON proposals(author_id, kind, quota_year);
CREATE INDEX proposals_collect_due ON proposals(status, collect_ends_at);
CREATE INDEX proposals_debate_due ON proposals(status, debate_ends_at);
CREATE INDEX proposals_vote_due ON proposals(status, vote_ends_at);
CREATE INDEX proposals_duplicates ON proposals(normalized_title);
CREATE INDEX proposals_duplicates_body ON proposals(normalized_body_hash);

-- Published text, authorship, kind and territory never change.
CREATE TRIGGER proposals_text_immutable BEFORE UPDATE OF
    title, summary, body, text_hash, kind, jurisdiction_id, author_id, proposed_setup_id
ON proposals
WHEN OLD.status <> 'draft'
BEGIN
    SELECT RAISE(ABORT, 'published proposal text is immutable');
END;
CREATE TRIGGER proposals_archived_final BEFORE UPDATE ON proposals
WHEN OLD.status = 'archived'
BEGIN
    SELECT RAISE(ABORT, 'archived proposal is final');
END;
CREATE TRIGGER proposals_published_no_delete BEFORE DELETE ON proposals
WHEN OLD.status <> 'draft'
BEGIN
    SELECT RAISE(ABORT, 'published proposal history is preserved');
END;

-- One row per account and proposal (the published text is the only version);
-- `active` toggles on withdrawal/re-signature during collection.
CREATE TABLE signatures (
    proposal_id TEXT NOT NULL REFERENCES proposals(id),
    user_id TEXT NOT NULL REFERENCES users(id),
    text_hash TEXT NOT NULL,
    active INTEGER NOT NULL CHECK (active IN (0, 1)),
    signed_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (proposal_id, user_id)
);
CREATE INDEX signatures_user ON signatures(user_id, active);

-- Client-supplied action keys: same key + payload returns the stored result,
-- same key with another payload is a conflict.
CREATE TABLE idempotency_keys (
    user_id TEXT NOT NULL REFERENCES users(id),
    key TEXT NOT NULL,
    action TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    response TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (user_id, key)
);
