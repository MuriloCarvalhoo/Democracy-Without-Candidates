//! AWS IAM-like authorization: a fixed action catalog, JSON policy documents and
//! their evaluation (explicit Deny > any Allow > implicit deny). Pure; the server
//! loads the documents attached to the caller and evaluates every request.
//! Account status, territory, authorship, phase and reauthentication are checked
//! separately by the use cases: an Allow never overrides them.

use serde::{Deserialize, Serialize};

use crate::i18n::t;

/// Only accepted `Version` of a policy document.
pub const POLICY_VERSION: &str = "2026-10-25";
pub const DOCUMENT_MAX_BYTES: usize = 8 * 1024;
pub const STATEMENTS_MAX: usize = 32;
pub const ENTRIES_MAX: usize = 64;
pub const NAME_MAX: usize = 64;
pub const DESCRIPTION_MAX: usize = 300;

/// Managed policies seeded by migration `0500_iam` (fixed ids, read-only).
pub const ADMINISTRATOR_ACCESS: &str = "pol-administrator-access";
pub const CIDADAO: &str = "pol-cidadao";
pub const MODERADOR: &str = "pol-moderador";
pub const AUDITOR: &str = "pol-auditor";
/// Applied to every request, including anonymous visitors (public reads).
pub const PUBLICO: &str = "pol-publico";

/// Reauthentication grant (face + TOTP) consumed by every IAM change.
pub const REAUTH_IAM: &str = "iam:manage";
pub const REAUTH_IAM_RESOURCE: &str = "iam";

/// Request resource of checks without resource-level scoping. Only the `*`
/// pattern matches it (as in IAM, scoped statements never grant global actions).
pub const ANY_RESOURCE: &str = "*";

/// `service:action` catalog. Documents naming anything else are rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    ProfileRead,
    ProfileUpdate,
    ProposalRead,
    ProposalCreate,
    ProposalSign,
    ProposalVote,
    ProposalArchive,
    CommentCreate,
    SetupStart,
    SetupVote,
    ModerationAct,
    FlagsManage,
    AdminRead,
    AuditRead,
    AuditReadRestricted,
    StatsRead,
    IamRead,
    IamManage,
}

impl Permission {
    pub const ALL: [Self; 18] = [
        Self::ProfileRead,
        Self::ProfileUpdate,
        Self::ProposalRead,
        Self::ProposalCreate,
        Self::ProposalSign,
        Self::ProposalVote,
        Self::ProposalArchive,
        Self::CommentCreate,
        Self::SetupStart,
        Self::SetupVote,
        Self::ModerationAct,
        Self::FlagsManage,
        Self::AdminRead,
        Self::AuditRead,
        Self::AuditReadRestricted,
        Self::StatsRead,
        Self::IamRead,
        Self::IamManage,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProfileRead => "profile:read",
            Self::ProfileUpdate => "profile:update",
            Self::ProposalRead => "proposal:read",
            Self::ProposalCreate => "proposal:create",
            Self::ProposalSign => "proposal:sign",
            Self::ProposalVote => "proposal:vote",
            Self::ProposalArchive => "proposal:archive",
            Self::CommentCreate => "comment:create",
            Self::SetupStart => "setup:start",
            Self::SetupVote => "setup:vote",
            Self::ModerationAct => "moderation:act",
            Self::FlagsManage => "flags:manage",
            Self::AdminRead => "admin:read",
            Self::AuditRead => "audit:read",
            Self::AuditReadRestricted => "audit:read_restricted",
            Self::StatsRead => "stats:read",
            Self::IamRead => "iam:read",
            Self::IamManage => "iam:manage",
        }
    }

    pub fn parse(action: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == action)
    }

    /// i18n key describing the action in the admin UI.
    pub fn label_key(self) -> &'static str {
        match self {
            Self::ProfileRead => "iam.action.profile_read",
            Self::ProfileUpdate => "iam.action.profile_update",
            Self::ProposalRead => "iam.action.proposal_read",
            Self::ProposalCreate => "iam.action.proposal_create",
            Self::ProposalSign => "iam.action.proposal_sign",
            Self::ProposalVote => "iam.action.proposal_vote",
            Self::ProposalArchive => "iam.action.proposal_archive",
            Self::CommentCreate => "iam.action.comment_create",
            Self::SetupStart => "iam.action.setup_start",
            Self::SetupVote => "iam.action.setup_vote",
            Self::ModerationAct => "iam.action.moderation_act",
            Self::FlagsManage => "iam.action.flags_manage",
            Self::AdminRead => "iam.action.admin_read",
            Self::AuditRead => "iam.action.audit_read",
            Self::AuditReadRestricted => "iam.action.audit_read_restricted",
            Self::StatsRead => "iam.action.stats_read",
            Self::IamRead => "iam.action.iam_read",
            Self::IamManage => "iam.action.iam_manage",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    Allow,
    Deny,
}

/// Validated statement: action and resource patterns already checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statement {
    pub sid: Option<String>,
    pub effect: Effect,
    pub actions: Vec<String>,
    pub resources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDocument {
    pub statements: Vec<Statement>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    fn into_vec(self) -> Vec<T> {
        match self {
            Self::One(v) => vec![v],
            Self::Many(v) => v,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDocument {
    #[serde(rename = "Version")]
    version: String,
    #[serde(rename = "Statement")]
    statement: OneOrMany<RawStatement>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStatement {
    #[serde(rename = "Sid", default)]
    sid: Option<String>,
    #[serde(rename = "Effect")]
    effect: Effect,
    #[serde(rename = "Action")]
    action: OneOrMany<String>,
    #[serde(rename = "Resource")]
    resource: OneOrMany<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocumentError {
    #[error("document too large")]
    TooLarge,
    #[error("invalid JSON: {0}")]
    Json(String),
    #[error("unsupported version")]
    Version,
    #[error("statement count")]
    Statements,
    #[error("empty or oversized list in statement {0}")]
    Entries(usize),
    #[error("invalid Sid in statement {0}")]
    Sid(usize),
    #[error("unknown action {0}")]
    UnknownAction(String),
    #[error("invalid resource {0}")]
    Resource(String),
}

impl DocumentError {
    /// pt-BR message for the policy editor.
    pub fn user_message(&self) -> String {
        match self {
            Self::TooLarge => t("iam.doc.too_large").into(),
            Self::Json(e) => format!("{} {e}", t("iam.doc.json")),
            Self::Version => format!("{} {POLICY_VERSION}", t("iam.doc.version")),
            Self::Statements => t("iam.doc.statements").into(),
            Self::Entries(i) => format!("{} {}", t("iam.doc.entries"), i + 1),
            Self::Sid(i) => format!("{} {}", t("iam.doc.sid"), i + 1),
            Self::UnknownAction(a) => format!("{} {a}", t("iam.doc.unknown_action")),
            Self::Resource(r) => format!("{} {r}", t("iam.doc.resource")),
        }
    }
}

/// `*`, an exact catalog action, or a prefix ending in `*` (`proposal:*`, `proposal:s*`).
pub fn action_matches(pattern: &str, action: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => action.starts_with(prefix),
        None => pattern == action,
    }
}

/// Same rule for resources; the request resource is `*` or `jurisdiction:<code>`.
pub fn resource_matches(pattern: &str, resource: &str) -> bool {
    action_matches(pattern, resource)
}

fn valid_action_pattern(pattern: &str) -> bool {
    let body = pattern.strip_suffix('*').unwrap_or(pattern);
    !body.contains('*')
        && body
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_' || c == ':')
        && Permission::ALL
            .iter()
            .any(|p| action_matches(pattern, p.as_str()))
}

/// `*`, `jurisdiction:*`, `jurisdiction:<IBGE code>` or a code prefix ending in `*`.
fn valid_resource_pattern(pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let Some(code) = pattern.strip_prefix("jurisdiction:") else {
        return false;
    };
    let body = code.strip_suffix('*').unwrap_or(code);
    (!body.is_empty() || code == "*")
        && body.len() <= 16
        && body.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Parses and validates a document (size, version, catalog actions, resources).
pub fn parse_document(json: &str) -> Result<PolicyDocument, DocumentError> {
    if json.len() > DOCUMENT_MAX_BYTES {
        return Err(DocumentError::TooLarge);
    }
    let raw: RawDocument =
        serde_json::from_str(json).map_err(|e| DocumentError::Json(e.to_string()))?;
    if raw.version != POLICY_VERSION {
        return Err(DocumentError::Version);
    }
    let raw_statements = raw.statement.into_vec();
    if raw_statements.is_empty() || raw_statements.len() > STATEMENTS_MAX {
        return Err(DocumentError::Statements);
    }
    let mut statements = Vec::with_capacity(raw_statements.len());
    for (i, s) in raw_statements.into_iter().enumerate() {
        let actions = s.action.into_vec();
        let resources = s.resource.into_vec();
        if actions.is_empty()
            || resources.is_empty()
            || actions.len() > ENTRIES_MAX
            || resources.len() > ENTRIES_MAX
        {
            return Err(DocumentError::Entries(i));
        }
        if let Some(sid) = &s.sid
            && (sid.len() > NAME_MAX || !sid.chars().all(|c| c.is_ascii_alphanumeric()))
        {
            return Err(DocumentError::Sid(i));
        }
        if let Some(bad) = actions.iter().find(|a| !valid_action_pattern(a)) {
            return Err(DocumentError::UnknownAction(bad.clone()));
        }
        if let Some(bad) = resources.iter().find(|r| !valid_resource_pattern(r)) {
            return Err(DocumentError::Resource(bad.clone()));
        }
        statements.push(Statement {
            sid: s.sid.filter(|s| !s.is_empty()),
            effect: s.effect,
            actions,
            resources,
        });
    }
    Ok(PolicyDocument { statements })
}

/// Policy name: 2-64 characters of letters, digits, `-` or `_`.
pub fn valid_name(name: &str) -> bool {
    (2..=NAME_MAX).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Statement that decided an evaluation, for the simulator and logs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub allowed: bool,
    /// `None` for the implicit deny (no statement matched).
    pub effect: Option<Effect>,
    pub policy: Option<String>,
    pub sid: Option<String>,
}

impl Decision {
    fn implicit_deny() -> Self {
        Self {
            allowed: false,
            effect: None,
            policy: None,
            sid: None,
        }
    }
}

/// Documents in force for one request, tagged with their policy name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Access {
    policies: Vec<(String, PolicyDocument)>,
}

impl Access {
    pub fn new(policies: Vec<(String, PolicyDocument)>) -> Self {
        Self { policies }
    }

    pub fn policy_names(&self) -> impl Iterator<Item = &str> {
        self.policies.iter().map(|(name, _)| name.as_str())
    }

    /// Explicit Deny wins over any Allow; no match is an implicit deny.
    pub fn evaluate(&self, action: Permission, resource: &str) -> Decision {
        let action = action.as_str();
        let mut allow = None;
        for (name, doc) in &self.policies {
            for s in &doc.statements {
                let matches = s.actions.iter().any(|a| action_matches(a, action))
                    && s.resources.iter().any(|r| resource_matches(r, resource));
                if !matches {
                    continue;
                }
                match s.effect {
                    Effect::Deny => {
                        return Decision {
                            allowed: false,
                            effect: Some(Effect::Deny),
                            policy: Some(name.clone()),
                            sid: s.sid.clone(),
                        };
                    }
                    Effect::Allow if allow.is_none() => allow = Some((name, s)),
                    Effect::Allow => {}
                }
            }
        }
        allow.map_or_else(Decision::implicit_deny, |(name, s)| Decision {
            allowed: true,
            effect: Some(Effect::Allow),
            policy: Some(name.clone()),
            sid: s.sid.clone(),
        })
    }

    pub fn allows(&self, action: Permission, resource: &str) -> bool {
        self.evaluate(action, resource).allowed
    }

    /// True when some statement names a specific resource, i.e. the decision may
    /// depend on the request resource (callers may then look it up).
    pub fn is_scoped(&self) -> bool {
        self.policies
            .iter()
            .flat_map(|(_, d)| &d.statements)
            .any(|s| s.resources.iter().any(|r| r != "*"))
    }

    /// Catalog actions allowed on every resource (`*`), for menus and summaries.
    pub fn allowed_actions(&self) -> Vec<Permission> {
        Permission::ALL
            .into_iter()
            .filter(|p| self.allows(*p, ANY_RESOURCE))
            .collect()
    }
}

/// Request resource for a jurisdiction-scoped check (IBGE code).
pub fn jurisdiction_resource(code: &str) -> String {
    format!("jurisdiction:{code}")
}

// Views shared by the admin pages and the server (no secrets, no addresses).

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicySummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub managed: bool,
    pub version: i64,
    /// Users plus groups the policy is attached to.
    pub attachments: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyView {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Pretty-printed JSON document.
    pub document: String,
    pub managed: bool,
    pub version: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRef {
    pub id: String,
    pub name: String,
    pub managed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub members: i64,
    pub policies: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserRef {
    pub id: String,
    pub email: String,
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub policies: Vec<PolicyRef>,
    pub members: Vec<UserRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserView {
    pub user: UserRef,
    pub policies: Vec<PolicyRef>,
    pub groups: Vec<GroupRef>,
    /// Effective actions on every resource (status, attachments and groups applied).
    pub allowed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Simulation {
    pub user: UserRef,
    pub action: String,
    pub resource: String,
    pub decision: Decision,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(json: &str) -> PolicyDocument {
        parse_document(json).unwrap()
    }

    fn access(docs: &[(&str, &str)]) -> Access {
        Access::new(
            docs.iter()
                .map(|(name, json)| ((*name).to_owned(), doc(json)))
                .collect(),
        )
    }

    const CITIZEN: &str = r#"{"Version":"2026-10-25","Statement":[{"Sid":"Participar","Effect":"Allow","Action":["proposal:*","comment:create"],"Resource":"*"}]}"#;

    #[test]
    fn catalog_round_trips_and_has_labels() {
        for p in Permission::ALL {
            assert_eq!(Permission::parse(p.as_str()), Some(p));
            assert!(p.label_key().starts_with("iam.action."));
        }
        assert_eq!(Permission::parse("proposal:delete"), None);
    }

    #[test]
    fn explicit_deny_wins_over_any_allow() {
        let a = access(&[
            (
                "Admin",
                r#"{"Version":"2026-10-25","Statement":{"Effect":"Allow","Action":"*","Resource":"*"}}"#,
            ),
            (
                "SemVoto",
                r#"{"Version":"2026-10-25","Statement":[{"Sid":"NoVote","Effect":"Deny","Action":"proposal:vote","Resource":"*"}]}"#,
            ),
        ]);
        let d = a.evaluate(Permission::ProposalVote, ANY_RESOURCE);
        assert!(!d.allowed);
        assert_eq!(d.effect, Some(Effect::Deny));
        assert_eq!(d.policy.as_deref(), Some("SemVoto"));
        assert_eq!(d.sid.as_deref(), Some("NoVote"));
        let d = a.evaluate(Permission::FlagsManage, ANY_RESOURCE);
        assert!(d.allowed);
        assert_eq!(d.policy.as_deref(), Some("Admin"));
    }

    #[test]
    fn no_match_is_implicit_deny() {
        let a = access(&[("Cidadao", CITIZEN)]);
        assert_eq!(
            a.evaluate(Permission::FlagsManage, ANY_RESOURCE),
            Decision::implicit_deny()
        );
        assert!(!Access::default().allows(Permission::ProposalRead, ANY_RESOURCE));
    }

    #[test]
    fn wildcards_match_services_and_prefixes() {
        let a = access(&[("Cidadao", CITIZEN)]);
        for p in [
            Permission::ProposalRead,
            Permission::ProposalVote,
            Permission::ProposalArchive,
            Permission::CommentCreate,
        ] {
            assert!(a.allows(p, ANY_RESOURCE), "{p:?}");
        }
        let a = access(&[(
            "Prefixo",
            r#"{"Version":"2026-10-25","Statement":[{"Effect":"Allow","Action":["audit:read*"],"Resource":["*"]}]}"#,
        )]);
        assert!(a.allows(Permission::AuditRead, ANY_RESOURCE));
        assert!(a.allows(Permission::AuditReadRestricted, ANY_RESOURCE));
        assert!(!a.allows(Permission::StatsRead, ANY_RESOURCE));
        assert_eq!(
            a.allowed_actions(),
            vec![Permission::AuditRead, Permission::AuditReadRestricted]
        );
    }

    #[test]
    fn resource_scoping_by_jurisdiction() {
        let a = access(&[(
            "ModeradorSC",
            r#"{"Version":"2026-10-25","Statement":[{"Effect":"Allow","Action":"moderation:act","Resource":["jurisdiction:42*","jurisdiction:3550308"]}]}"#,
        )]);
        assert!(a.is_scoped());
        assert!(a.allows(Permission::ModerationAct, &jurisdiction_resource("4202404")));
        assert!(a.allows(Permission::ModerationAct, &jurisdiction_resource("42")));
        assert!(a.allows(Permission::ModerationAct, &jurisdiction_resource("3550308")));
        assert!(!a.allows(Permission::ModerationAct, &jurisdiction_resource("3304557")));
        assert!(
            !a.allows(Permission::ModerationAct, ANY_RESOURCE),
            "scoped statements never grant unscoped checks"
        );
        assert!(!access(&[("Cidadao", CITIZEN)]).is_scoped());
        // A scoped Deny only removes that jurisdiction.
        let a = access(&[
            ("Cidadao", CITIZEN),
            (
                "SemRJ",
                r#"{"Version":"2026-10-25","Statement":[{"Effect":"Deny","Action":"proposal:*","Resource":"jurisdiction:33*"}]}"#,
            ),
        ]);
        assert!(!a.allows(
            Permission::ProposalCreate,
            &jurisdiction_resource("3304557")
        ));
        assert!(a.allows(
            Permission::ProposalCreate,
            &jurisdiction_resource("4202404")
        ));
    }

    #[test]
    fn invalid_documents_are_rejected() {
        let v = POLICY_VERSION;
        let cases = [
            (
                format!(r#"{{"Version":"{v}","Statement":[{{"Effect":"Allow","Action":"proposal:delete","Resource":"*"}}]}}"#),
                DocumentError::UnknownAction("proposal:delete".into()),
            ),
            (
                format!(r#"{{"Version":"{v}","Statement":[{{"Effect":"Allow","Action":"nada:*","Resource":"*"}}]}}"#),
                DocumentError::UnknownAction("nada:*".into()),
            ),
            (
                format!(r#"{{"Version":"{v}","Statement":[{{"Effect":"Allow","Action":"pro*posal:read","Resource":"*"}}]}}"#),
                DocumentError::UnknownAction("pro*posal:read".into()),
            ),
            (
                format!(r#"{{"Version":"{v}","Statement":[{{"Effect":"Allow","Action":"*","Resource":"arn:x"}}]}}"#),
                DocumentError::Resource("arn:x".into()),
            ),
            (
                format!(r#"{{"Version":"{v}","Statement":[{{"Effect":"Allow","Action":"*","Resource":"jurisdiction:"}}]}}"#),
                DocumentError::Resource("jurisdiction:".into()),
            ),
            (
                r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"*","Resource":"*"}]}"#.into(),
                DocumentError::Version,
            ),
            (
                format!(r#"{{"Version":"{v}","Statement":[]}}"#),
                DocumentError::Statements,
            ),
            (
                format!(r#"{{"Version":"{v}","Statement":[{{"Effect":"Allow","Action":[],"Resource":"*"}}]}}"#),
                DocumentError::Entries(0),
            ),
            (
                format!(r#"{{"Version":"{v}","Statement":[{{"Sid":"a b","Effect":"Allow","Action":"*","Resource":"*"}}]}}"#),
                DocumentError::Sid(0),
            ),
        ];
        for (json, expected) in cases {
            assert_eq!(parse_document(&json), Err(expected), "{json}");
        }
        // Unsupported IAM features (Condition, NotAction) and unknown effects fail.
        for json in [
            format!(
                r#"{{"Version":"{v}","Statement":[{{"Effect":"Allow","Action":"*","Resource":"*","Condition":{{}}}}]}}"#
            ),
            format!(
                r#"{{"Version":"{v}","Statement":[{{"Effect":"Maybe","Action":"*","Resource":"*"}}]}}"#
            ),
            "not json".into(),
        ] {
            assert!(
                matches!(parse_document(&json), Err(DocumentError::Json(_))),
                "{json}"
            );
        }
        let big = format!(
            r#"{{"Version":"{v}","Statement":[{{"Sid":"{}","Effect":"Allow","Action":"*","Resource":"*"}}]}}"#,
            "a".repeat(DOCUMENT_MAX_BYTES)
        );
        assert_eq!(parse_document(&big), Err(DocumentError::TooLarge));
    }

    #[test]
    fn names_are_restricted() {
        assert!(valid_name("Moderacao-SC_1"));
        assert!(!valid_name("a"));
        assert!(!valid_name("com espaço"));
        assert!(!valid_name(&"a".repeat(65)));
    }
}
