-- Identity core shared by all features: accounts, immutable address, sessions,
-- reauthentication grants and in-app notifications.

CREATE TABLE users (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'active', 'blocked')),
    municipality_id TEXT NOT NULL REFERENCES jurisdictions(id),
    state_id TEXT NOT NULL REFERENCES jurisdictions(id),
    country_id TEXT NOT NULL REFERENCES jurisdictions(id),
    synthetic INTEGER NOT NULL DEFAULT 0 CHECK (synthetic IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX users_municipality ON users(municipality_id, status);
CREATE INDEX users_state ON users(state_id, status);

-- Territory is derived from the initial address and cannot change in this version.
CREATE TRIGGER users_territory_immutable BEFORE UPDATE OF municipality_id, state_id, country_id ON users
WHEN NEW.municipality_id IS NOT OLD.municipality_id
  OR NEW.state_id IS NOT OLD.state_id
  OR NEW.country_id IS NOT OLD.country_id
BEGIN
    SELECT RAISE(ABORT, 'territory is immutable');
END;

CREATE TABLE user_roles (
    user_id TEXT NOT NULL REFERENCES users(id),
    role TEXT NOT NULL CHECK (role IN ('CITIZEN', 'AUDITOR', 'COUNTRY_ADMIN')),
    granted_at TEXT NOT NULL,
    granted_by TEXT,
    PRIMARY KEY (user_id, role)
);

-- BrasilAPI field names are preserved; ibge is the serialized nested object.
CREATE TABLE addresses (
    user_id TEXT PRIMARY KEY REFERENCES users(id),
    cep TEXT NOT NULL,
    state TEXT NOT NULL,
    city TEXT NOT NULL,
    neighborhood TEXT NOT NULL,
    street TEXT NOT NULL,
    service TEXT NOT NULL,
    ibge TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TRIGGER addresses_no_update BEFORE UPDATE ON addresses
BEGIN
    SELECT RAISE(ABORT, 'address is immutable');
END;
CREATE TRIGGER addresses_no_delete BEFORE DELETE ON addresses
BEGIN
    SELECT RAISE(ABORT, 'address is immutable');
END;

-- Only the SHA-256 of the opaque cookie value is stored.
CREATE TABLE sessions (
    id_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    idle_expires_at TEXT NOT NULL,
    absolute_expires_at TEXT NOT NULL,
    revoked_at TEXT,
    revoke_reason TEXT
);
CREATE INDEX sessions_user ON sessions(user_id, revoked_at);

-- Face + TOTP proof bound to session, action and resource; valid 5 minutes, single use.
CREATE TABLE reauth_grants (
    id TEXT PRIMARY KEY,
    session_hash TEXT NOT NULL REFERENCES sessions(id_hash),
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT
);
CREATE INDEX reauth_grants_lookup ON reauth_grants(session_hash, action, resource);

CREATE TABLE notifications (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    event TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    link TEXT,
    created_at TEXT NOT NULL,
    read_at TEXT
);
CREATE INDEX notifications_user ON notifications(user_id, read_at, created_at);
