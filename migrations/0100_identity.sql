-- Identity: authentication factors, recovery codes, multi-step login state,
-- single-use face challenges, failure counters and privacy preferences.

-- Face template and TOTP secrets are encrypted by the application (XChaCha20-Poly1305,
-- key outside the database). `totp_last_step` blocks replay of an accepted step.
CREATE TABLE user_credentials (
    user_id TEXT PRIMARY KEY REFERENCES users(id),
    face_template BLOB,
    face_enrolled_at TEXT,
    totp_secret BLOB,
    totp_confirmed_at TEXT,
    totp_last_step INTEGER,
    totp_pending_secret BLOB,
    totp_reenroll_required INTEGER NOT NULL DEFAULT 0 CHECK (totp_reenroll_required IN (0, 1)),
    updated_at TEXT NOT NULL
);

-- Only SHA-256 of each 128-bit code is stored; a code is consumed once.
CREATE TABLE recovery_codes (
    code_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    used_at TEXT,
    revoked_at TEXT
);
CREATE INDEX recovery_codes_user ON recovery_codes(user_id, used_at, revoked_at);

-- State between the steps of login/password reset, bound to an HttpOnly cookie
-- (hash only), single use and short-lived. user_id is NULL for unknown e-mails so
-- the flow behaves the same without revealing accounts.
CREATE TABLE auth_challenges (
    token_hash TEXT PRIMARY KEY,
    purpose TEXT NOT NULL CHECK (purpose IN ('login', 'password_reset')),
    user_id TEXT REFERENCES users(id),
    account_key TEXT NOT NULL,
    password_ok INTEGER NOT NULL DEFAULT 0 CHECK (password_ok IN (0, 1)),
    face_ok INTEGER NOT NULL DEFAULT 0 CHECK (face_ok IN (0, 1)),
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT
);

-- Single-use nonce for one face capture, bound to a subject (user or auth challenge).
CREATE TABLE face_challenges (
    challenge_hash TEXT PRIMARY KEY,
    subject TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT
);

-- Recent authentication failures for rate limiting. `key` is a SHA-256 of the
-- normalized e-mail or client IP; rows older than the window are pruned.
CREATE TABLE auth_failures (
    id TEXT PRIMARY KEY,
    scope TEXT NOT NULL CHECK (scope IN ('account', 'ip')),
    key TEXT NOT NULL,
    occurred_at TEXT NOT NULL
);
CREATE INDEX auth_failures_lookup ON auth_failures(scope, key, occurred_at);

CREATE TABLE user_preferences (
    user_id TEXT PRIMARY KEY REFERENCES users(id),
    profile_visibility TEXT NOT NULL DEFAULT 'private' CHECK (profile_visibility IN ('public', 'private')),
    default_vote_visibility TEXT NOT NULL DEFAULT 'private' CHECK (default_vote_visibility IN ('public', 'private')),
    updated_at TEXT NOT NULL
);
