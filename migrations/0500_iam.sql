-- AWS IAM-like authorization: policy documents attached to users and groups.
-- Replaces user_roles (migrated below and dropped). Managed policies are seeded
-- here and are read-only; customer policies are created by `iam:manage` holders.

CREATE TABLE iam_policies (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    document TEXT NOT NULL,
    managed INTEGER NOT NULL DEFAULT 0 CHECK (managed IN (0, 1)),
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    created_by TEXT
);
CREATE TRIGGER iam_policies_managed_no_update BEFORE UPDATE ON iam_policies
WHEN OLD.managed = 1
BEGIN
    SELECT RAISE(ABORT, 'managed policies are read-only');
END;
CREATE TRIGGER iam_policies_managed_no_delete BEFORE DELETE ON iam_policies
WHEN OLD.managed = 1
BEGIN
    SELECT RAISE(ABORT, 'managed policies cannot be deleted');
END;

CREATE TABLE iam_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    created_by TEXT
);

CREATE TABLE iam_group_policies (
    group_id TEXT NOT NULL REFERENCES iam_groups(id),
    policy_id TEXT NOT NULL REFERENCES iam_policies(id),
    attached_at TEXT NOT NULL,
    attached_by TEXT,
    PRIMARY KEY (group_id, policy_id)
);
CREATE INDEX iam_group_policies_policy ON iam_group_policies(policy_id);

CREATE TABLE iam_user_groups (
    user_id TEXT NOT NULL REFERENCES users(id),
    group_id TEXT NOT NULL REFERENCES iam_groups(id),
    added_at TEXT NOT NULL,
    added_by TEXT,
    PRIMARY KEY (user_id, group_id)
);
CREATE INDEX iam_user_groups_group ON iam_user_groups(group_id);

CREATE TABLE iam_user_policies (
    user_id TEXT NOT NULL REFERENCES users(id),
    policy_id TEXT NOT NULL REFERENCES iam_policies(id),
    attached_at TEXT NOT NULL,
    attached_by TEXT,
    PRIMARY KEY (user_id, policy_id)
);
CREATE INDEX iam_user_policies_policy ON iam_user_policies(policy_id);

INSERT INTO iam_policies (id, name, description, document, managed, created_at, updated_at, created_by) VALUES
('pol-administrator-access', 'AdministratorAccess', 'Acesso total à instância (admin master).',
 '{"Version":"2026-10-25","Statement":[{"Sid":"TudoLiberado","Effect":"Allow","Action":"*","Resource":"*"}]}',
 1, '2026-10-25T00:00:00.000Z', '2026-10-25T00:00:00.000Z', 'migration:0500_iam'),
('pol-cidadao', 'Cidadao', 'Participação cidadã: ler, propor, assinar, votar e comentar. Anexada a toda conta ativa.',
 '{"Version":"2026-10-25","Statement":[{"Sid":"Participacao","Effect":"Allow","Action":["profile:read","profile:update","proposal:read","proposal:create","proposal:sign","proposal:vote","comment:create","setup:vote","audit:read","stats:read"],"Resource":"*"}]}',
 1, '2026-10-25T00:00:00.000Z', '2026-10-25T00:00:00.000Z', 'migration:0500_iam'),
('pol-moderador', 'Moderador', 'Moderação do debate: fila de denúncias, ocultar/restaurar comentários e travar debates.',
 '{"Version":"2026-10-25","Statement":[{"Sid":"Moderacao","Effect":"Allow","Action":"moderation:act","Resource":"*"}]}',
 1, '2026-10-25T00:00:00.000Z', '2026-10-25T00:00:00.000Z', 'migration:0500_iam'),
('pol-auditor', 'Auditor', 'Leitura dos eventos restritos de auditoria (cada leitura é auditada).',
 '{"Version":"2026-10-25","Statement":[{"Sid":"Auditoria","Effect":"Allow","Action":["audit:read","audit:read_restricted"],"Resource":"*"}]}',
 1, '2026-10-25T00:00:00.000Z', '2026-10-25T00:00:00.000Z', 'migration:0500_iam'),
('pol-publico', 'Publico', 'Leituras públicas de transparência; vale para visitantes e para toda conta.',
 '{"Version":"2026-10-25","Statement":[{"Sid":"Transparencia","Effect":"Allow","Action":["proposal:read","audit:read","stats:read"],"Resource":"*"}]}',
 1, '2026-10-25T00:00:00.000Z', '2026-10-25T00:00:00.000Z', 'migration:0500_iam');

-- CITIZEN -> Cidadao, AUDITOR -> Auditor, COUNTRY_ADMIN -> AdministratorAccess.
INSERT INTO iam_user_policies (user_id, policy_id, attached_at, attached_by)
SELECT user_id,
       CASE role WHEN 'CITIZEN' THEN 'pol-cidadao' WHEN 'AUDITOR' THEN 'pol-auditor' ELSE 'pol-administrator-access' END,
       granted_at,
       COALESCE(granted_by, 'migration:0500_iam')
FROM user_roles;

DROP TABLE user_roles;
