//! Notification categories and views shared by server and browser. No I/O.

use serde::{Deserialize, Serialize};

/// Categories a citizen can enable/disable. Events outside this list (e.g.
/// security notices) are always delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Category {
    ProposalCreated,
    VotingOpened,
    DeadlineReminder,
    ResultPublished,
    PhaseChanged,
}

impl Category {
    pub const ALL: [Category; 5] = [
        Self::ProposalCreated,
        Self::VotingOpened,
        Self::DeadlineReminder,
        Self::ResultPublished,
        Self::PhaseChanged,
    ];

    /// Value stored in `notifications.event` and preference rows.
    pub fn as_db(self) -> &'static str {
        match self {
            Self::ProposalCreated => "proposal_created",
            Self::VotingOpened => "voting_opened",
            Self::DeadlineReminder => "deadline_reminder",
            Self::ResultPublished => "result_published",
            Self::PhaseChanged => "phase_changed",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_db() == value)
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::ProposalCreated => "notifications.category.proposal_created",
            Self::VotingOpened => "notifications.category.voting_opened",
            Self::DeadlineReminder => "notifications.category.deadline_reminder",
            Self::ResultPublished => "notifications.category.result_published",
            Self::PhaseChanged => "notifications.category.phase_changed",
        }
    }
}

pub const PAGE_SIZE: u32 = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationView {
    pub id: String,
    pub event: String,
    pub title: String,
    pub body: String,
    pub link: Option<String>,
    pub created_at: String,
    pub read: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationPage {
    pub items: Vec<NotificationView>,
    pub unread: i64,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preference {
    pub category: Category,
    pub in_app: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_round_trip() {
        for c in Category::ALL {
            assert_eq!(Category::from_db(c.as_db()), Some(c));
        }
        assert_eq!(Category::from_db("security_notice"), None);
    }
}
