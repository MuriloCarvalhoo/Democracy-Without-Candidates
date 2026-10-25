-- Demonstrative PUBLIC ballots and closed results. Secret ballots are blocked
-- until a validated protocol exists: `visibility` only admits 'public' and every
-- ballot is linked to the account that cast it with explicit consent.

-- One accepted ballot per proposal and account (UNIQUE + checks in the cast
-- transaction). Append-only: a ballot is never edited or removed.
CREATE TABLE ballots (
    id TEXT PRIMARY KEY,
    proposal_id TEXT NOT NULL REFERENCES proposals(id),
    user_id TEXT NOT NULL REFERENCES users(id),
    snapshot_id TEXT NOT NULL REFERENCES electorate_snapshots(id),
    choice TEXT NOT NULL CHECK (choice IN ('yes', 'no', 'abstain')),
    visibility TEXT NOT NULL CHECK (visibility = 'public'),
    consented_at TEXT NOT NULL,
    cast_at TEXT NOT NULL,
    UNIQUE (proposal_id, user_id)
);
CREATE INDEX ballots_tally ON ballots(proposal_id, choice);
CREATE TRIGGER ballots_no_update BEFORE UPDATE ON ballots
BEGIN
    SELECT RAISE(ABORT, 'ballots are append-only');
END;
CREATE TRIGGER ballots_no_delete BEFORE DELETE ON ballots
BEGIN
    SELECT RAISE(ABORT, 'ballots are append-only');
END;

-- Result written in the closing transaction from the ballots and the rules
-- captured at voting opening. Immutable; reconstructible from `ballots`.
CREATE TABLE vote_results (
    proposal_id TEXT PRIMARY KEY REFERENCES proposals(id),
    snapshot_id TEXT NOT NULL REFERENCES electorate_snapshots(id),
    setup_version_id TEXT NOT NULL REFERENCES setup_versions(id),
    electorate INTEGER NOT NULL CHECK (electorate > 0),
    yes_count INTEGER NOT NULL CHECK (yes_count >= 0),
    no_count INTEGER NOT NULL CHECK (no_count >= 0),
    abstain_count INTEGER NOT NULL CHECK (abstain_count >= 0),
    quorum_bp INTEGER NOT NULL,
    majority_bp INTEGER NOT NULL,
    quorum_required INTEGER NOT NULL,
    majority_required INTEGER NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('approved', 'rejected', 'quorum_not_reached')),
    reason TEXT CHECK (reason IN ('no_valid_votes', 'tie', 'majority_not_reached')),
    closed_at TEXT NOT NULL,
    CHECK ((outcome = 'rejected') = (reason IS NOT NULL)),
    CHECK (yes_count + no_count + abstain_count <= electorate)
);
CREATE TRIGGER vote_results_no_update BEFORE UPDATE ON vote_results
BEGIN
    SELECT RAISE(ABORT, 'vote results are immutable');
END;
CREATE TRIGGER vote_results_no_delete BEFORE DELETE ON vote_results
BEGIN
    SELECT RAISE(ABORT, 'vote results are immutable');
END;
