//! Debate rules and views shared by server and browser: comments, reports,
//! moderation and amendments. Limits are agent proposals for review. No I/O.

use serde::{Deserialize, Serialize};

use super::proposals::ProposalStatus;

/// Maximum nesting: top-level comment (0) and two reply levels (1, 2).
pub const MAX_DEPTH: u8 = 2;
pub const COMMENT_MAX: usize = 2_000;
pub const AMENDMENT_TITLE_MAX: usize = 160;
pub const AMENDMENT_BODY_MAX: usize = 5_000;
pub const DETAILS_MAX: usize = 500;
pub const REASON_MIN: usize = 5;
pub const REASON_MAX: usize = 500;
/// Top-level threads per page.
pub const THREADS_PER_PAGE: u32 = 20;
/// Comment writes (new comments + edits) per account in [`RATE_WINDOW_MINUTES`].
pub const RATE_LIMIT: u32 = 10;
pub const RATE_WINDOW_MINUTES: i64 = 10;
/// Amendments per account in 24 hours.
pub const AMENDMENT_DAILY_LIMIT: u32 = 5;

/// Comments are accepted from signature collection until voting closes.
pub fn comments_open(status: ProposalStatus) -> bool {
    matches!(
        status,
        ProposalStatus::CollectingSignatures | ProposalStatus::InDebate | ProposalStatus::InVoting
    )
}

/// Amendments refer to the text before the vote: collection and debate only.
pub fn amendments_open(status: ProposalStatus) -> bool {
    matches!(
        status,
        ProposalStatus::CollectingSignatures | ProposalStatus::InDebate
    )
}

/// Why the current viewer cannot post (shown instead of the form).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PostBlock {
    Visitor,
    NotEligible,
    PhaseClosed,
    Locked,
}

impl PostBlock {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Visitor => "debate.block.visitor",
            Self::NotEligible => "debate.block.not_eligible",
            Self::PhaseClosed => "debate.block.phase_closed",
            Self::Locked => "debate.block.locked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReportReason {
    Spam,
    Offensive,
    OffTopic,
    Misinformation,
    Other,
}

impl ReportReason {
    pub const ALL: [Self; 5] = [
        Self::Spam,
        Self::Offensive,
        Self::OffTopic,
        Self::Misinformation,
        Self::Other,
    ];

    pub fn as_db(self) -> &'static str {
        match self {
            Self::Spam => "spam",
            Self::Offensive => "offensive",
            Self::OffTopic => "off_topic",
            Self::Misinformation => "misinformation",
            Self::Other => "other",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_db() == value)
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Spam => "debate.report.reason.spam",
            Self::Offensive => "debate.report.reason.offensive",
            Self::OffTopic => "debate.report.reason.off_topic",
            Self::Misinformation => "debate.report.reason.misinformation",
            Self::Other => "debate.report.reason.other",
        }
    }
}

/// Moderator decision on a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModerationDecision {
    /// Hide from the public view (content kept) and resolve open reports.
    Remove,
    /// Show again a removed comment.
    Restore,
    /// Close open reports without hiding the comment.
    Dismiss,
}

impl ModerationDecision {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::Remove => "remove",
            Self::Restore => "restore",
            Self::Dismiss => "dismiss_report",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmendmentStatus {
    Open,
    Withdrawn,
    Addressed,
}

impl AmendmentStatus {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Withdrawn => "withdrawn",
            Self::Addressed => "addressed",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "withdrawn" => Some(Self::Withdrawn),
            "addressed" => Some(Self::Addressed),
            _ => None,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Open => "debate.amendment.status.open",
            Self::Withdrawn => "debate.amendment.status.withdrawn",
            Self::Addressed => "debate.amendment.status.addressed",
        }
    }
}

/// Change requested on an open amendment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmendmentChange {
    /// By the amendment author.
    Withdraw,
    /// By the proposal author, with a note (e.g. "will become a new proposal").
    Address,
}

pub mod view {
    use serde::{Deserialize, Serialize};

    use super::{AmendmentStatus, PostBlock, ReportReason};
    use crate::domain::proposals::ProposalStatus;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct LockInfo {
        pub reason: String,
        pub at: String,
    }

    /// Comment in display order (threads flattened depth-first).
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct CommentView {
        pub id: String,
        pub parent_id: Option<String>,
        pub depth: u8,
        /// Public name, or a neutral label when the profile is private.
        pub author: String,
        pub own: bool,
        /// `None` for removed comments, except in the moderator view.
        pub body: Option<String>,
        pub removed: bool,
        pub edited_at: Option<String>,
        pub created_at: String,
        pub can_reply: bool,
        pub can_edit: bool,
        pub can_report: bool,
        pub reported: bool,
        /// Open reports (moderator view only).
        pub open_reports: Option<u32>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct DebateViewer {
        /// `None` when the viewer can post.
        pub block: Option<PostBlock>,
        pub moderator: bool,
        pub proposal_author: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct DebateView {
        pub proposal_id: String,
        pub title: String,
        pub status: ProposalStatus,
        pub lock: Option<LockInfo>,
        pub comments: Vec<CommentView>,
        pub page: u32,
        pub pages: u32,
        pub threads: u64,
        pub amendments: u64,
        pub viewer: Option<DebateViewer>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Revision {
        pub rev: u32,
        pub body: String,
        pub at: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct AmendmentView {
        pub id: String,
        pub title: String,
        pub body: String,
        pub author: String,
        pub own: bool,
        pub status: AmendmentStatus,
        pub status_note: Option<String>,
        pub created_at: String,
        pub text_hash: String,
        /// False when it refers to another text hash than the published one.
        pub current_text: bool,
        pub can_withdraw: bool,
        pub can_address: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct AmendmentsView {
        pub proposal_id: String,
        pub title: String,
        pub status: ProposalStatus,
        pub text_hash: String,
        pub items: Vec<AmendmentView>,
        /// `None` when the viewer can propose (visitors get `Visitor`).
        pub block: Option<PostBlock>,
    }

    /// Reported comment in the moderation queue.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ReportedComment {
        pub comment_id: String,
        pub proposal_id: String,
        pub proposal_title: String,
        pub body: String,
        pub removed: bool,
        pub reasons: Vec<(ReportReason, u32)>,
        pub details: Vec<String>,
        pub first_at: String,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_accepting_posts() {
        use ProposalStatus::*;
        for s in ProposalStatus::ALL {
            assert_eq!(
                comments_open(*s),
                matches!(s, CollectingSignatures | InDebate | InVoting),
                "{s:?}"
            );
            if amendments_open(*s) {
                assert!(comments_open(*s));
            }
        }
        assert!(!amendments_open(InVoting));
        assert!(!comments_open(Draft));
    }

    #[test]
    fn report_reasons_round_trip() {
        for r in ReportReason::ALL {
            assert_eq!(ReportReason::from_db(r.as_db()), Some(r));
        }
        assert_eq!(ReportReason::from_db("upvote"), None);
    }
}
