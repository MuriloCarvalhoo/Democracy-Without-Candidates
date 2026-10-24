use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    Citizen,
    Auditor,
    CountryAdmin,
}

impl Role {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::Citizen => "CITIZEN",
            Self::Auditor => "AUDITOR",
            Self::CountryAdmin => "COUNTRY_ADMIN",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "CITIZEN" => Some(Self::Citizen),
            "AUDITOR" => Some(Self::Auditor),
            "COUNTRY_ADMIN" => Some(Self::CountryAdmin),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountStatus {
    /// PENDING_CITIZEN: registered, face/TOTP enrollment incomplete.
    Pending,
    Active,
    Blocked,
}

impl AccountStatus {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Blocked => "blocked",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "active" => Some(Self::Active),
            "blocked" => Some(Self::Blocked),
            _ => None,
        }
    }
}

/// `resource:action` permissions. Territory, authorship and phase are checked
/// separately by the use case; a permission alone never authorizes an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    ProfileRead,
    ProfileUpdate,
    ProposalCreate,
    ProposalSign,
    ProposalVote,
    CommentCreate,
    SetupVote,
    SetupStart,
    ModerationAct,
    ProposalArchive,
    FlagsManage,
    AuditorsManage,
    AuditReadRestricted,
}

pub fn permissions(status: AccountStatus, roles: &[Role]) -> Vec<Permission> {
    use Permission::*;
    match status {
        AccountStatus::Blocked => return Vec::new(),
        AccountStatus::Pending => return vec![ProfileRead],
        AccountStatus::Active => {}
    }
    let mut out = vec![ProfileRead];
    for role in roles {
        let granted: &[Permission] = match role {
            Role::Citizen => &[
                ProfileUpdate,
                ProposalCreate,
                ProposalSign,
                ProposalVote,
                CommentCreate,
                SetupVote,
            ],
            Role::Auditor => &[AuditReadRestricted],
            Role::CountryAdmin => &[
                SetupStart,
                ModerationAct,
                ProposalArchive,
                FlagsManage,
                AuditorsManage,
                AuditReadRestricted,
            ],
        };
        for p in granted {
            if !out.contains(p) {
                out.push(*p);
            }
        }
    }
    out
}

/// Email used for per-instance uniqueness: trimmed and lowercased, no provider aliases.
pub fn normalize_email(raw: &str) -> Option<String> {
    let email = raw.trim().to_lowercase();
    let (local, domain) = email.split_once('@')?;
    let valid = !local.is_empty()
        && local.len() <= 64
        && email.len() <= 254
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.chars().any(|c| c.is_whitespace() || c.is_control())
        && !domain.contains('@');
    valid.then_some(email)
}

pub const PASSWORD_MIN: usize = 12;
pub const PASSWORD_MAX: usize = 128;

/// Length-only policy (12–128 characters), no artificial composition rules.
pub fn password_acceptable(password: &str) -> bool {
    (PASSWORD_MIN..=PASSWORD_MAX).contains(&password.chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_normalization() {
        assert_eq!(
            normalize_email("  Ana@Example.ORG "),
            Some("ana@example.org".into())
        );
        assert_eq!(
            normalize_email("ana+x@example.org"),
            Some("ana+x@example.org".into())
        );
        for bad in [
            "",
            "ana",
            "@example.org",
            "ana@",
            "ana@example",
            "a b@example.org",
            "ana@@example.org",
        ] {
            assert_eq!(normalize_email(bad), None, "{bad}");
        }
    }

    #[test]
    fn password_length_policy() {
        assert!(!password_acceptable("short"));
        assert!(password_acceptable("correct horse"));
        assert!(password_acceptable(&"é".repeat(12)));
        assert!(!password_acceptable(&"a".repeat(129)));
    }

    #[test]
    fn pending_and_blocked_cannot_participate() {
        assert_eq!(
            permissions(AccountStatus::Pending, &[Role::Citizen]),
            vec![Permission::ProfileRead]
        );
        assert!(
            permissions(AccountStatus::Blocked, &[Role::Citizen, Role::CountryAdmin]).is_empty()
        );
        let citizen = permissions(AccountStatus::Active, &[Role::Citizen]);
        assert!(citizen.contains(&Permission::ProposalVote));
        assert!(!citizen.contains(&Permission::FlagsManage));
        let admin = permissions(AccountStatus::Active, &[Role::CountryAdmin]);
        assert!(
            !admin.contains(&Permission::ProposalVote),
            "admin role alone does not vote"
        );
    }
}
