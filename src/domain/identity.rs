use serde::{Deserialize, Serialize};

/// Authorization moved to IAM policies (`domain::iam`); re-exported for use cases.
pub use super::iam::Permission;

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

pub const NAME_MAX: usize = 120;

/// Display name: trimmed, inner whitespace collapsed, 2–120 characters, no controls.
pub fn normalize_name(raw: &str) -> Option<String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let len = name.chars().count();
    ((2..=NAME_MAX).contains(&len) && !name.chars().any(char::is_control)).then_some(name)
}

/// Profile and default vote visibility chosen by the citizen. Private by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    #[default]
    Private,
}

impl Visibility {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "public" => Some(Self::Public),
            "private" => Some(Self::Private),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_normalization() {
        assert_eq!(
            normalize_name("  Ana   Maria ").as_deref(),
            Some("Ana Maria")
        );
        assert_eq!(normalize_name("A"), None);
        assert_eq!(normalize_name(&"a".repeat(121)), None);
        assert_eq!(Visibility::default(), Visibility::Private);
    }

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
}
