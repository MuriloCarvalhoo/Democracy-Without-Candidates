-- Indexes for expiry cleanups and the national electorate lookup (the column
-- is chosen from the jurisdiction kind, see application::territory).
CREATE INDEX face_challenges_expires ON face_challenges(expires_at);
CREATE INDEX auth_challenges_expires ON auth_challenges(expires_at);
CREATE INDEX auth_failures_occurred ON auth_failures(occurred_at);
CREATE INDEX users_country ON users(country_id, status);

-- Accepted ballots per proposal, incremented in the cast transaction so a vote
-- does not count every ballot under the single writer. Backfilled from ballots.
CREATE TABLE ballot_counts (
    proposal_id TEXT PRIMARY KEY REFERENCES proposals(id),
    ballots INTEGER NOT NULL CHECK (ballots >= 0)
);
INSERT INTO ballot_counts (proposal_id, ballots)
    SELECT proposal_id, COUNT(*) FROM ballots GROUP BY proposal_id;
