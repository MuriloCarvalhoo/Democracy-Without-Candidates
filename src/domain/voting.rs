//! Demonstrative public vote: choices, display arithmetic and shared views.
//! Secret ballots are blocked until a validated protocol exists (flag
//! `voting.private` can never be enabled); nothing here claims secrecy.

use serde::{Deserialize, Serialize};

use super::proposals::{Outcome, ProposalKind, Tally, view::ProposalSummary};
use super::setup::BP_FULL;

/// Reauthentication actions, bound to the proposal id as resource.
pub const REAUTH_VOTE: &str = "proposal:vote";
/// Setup votes: initial plebiscite and meta-laws (`setup_change`).
pub const REAUTH_SETUP_VOTE: &str = "setup:vote";

/// Smallest published count of accounts per territory in statistics.
pub const MIN_CELL: u64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Choice {
    Yes,
    No,
    Abstain,
}

impl Choice {
    pub const ALL: [Choice; 3] = [Self::Yes, Self::No, Self::Abstain];

    pub fn as_db(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
            Self::Abstain => "abstain",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_db() == value)
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Yes => "voting.choice.yes",
            Self::No => "voting.choice.no",
            Self::Abstain => "voting.choice.abstain",
        }
    }
}

/// Setup plebiscites and meta-laws use the setup vote permission/action.
pub fn is_setup_vote(kind: ProposalKind) -> bool {
    matches!(kind, ProposalKind::SetupChange | ProposalKind::InitialSetup)
}

pub fn reauth_action(kind: ProposalKind) -> &'static str {
    if is_setup_vote(kind) {
        REAUTH_SETUP_VOTE
    } else {
        REAUTH_VOTE
    }
}

/// Display share in basis points, rounded half up: round(10000 × part / total).
/// `None` when `total` is zero. Decisions never use this value (see
/// `proposals::compute_result`, which works on exact integer counts).
pub fn share_bp(part: u64, total: u64) -> Option<u32> {
    if total == 0 {
        return None;
    }
    let num = 2 * u128::from(part) * u128::from(BP_FULL) + u128::from(total);
    Some((num / (2 * u128::from(total))) as u32)
}

/// "52,34%" (pt-BR decimal comma), from basis points.
pub fn format_bp(bp: u32) -> String {
    format!("{},{:02}%", bp / 100, bp % 100)
}

pub fn format_share(part: u64, total: u64) -> String {
    share_bp(part, total)
        .map(format_bp)
        .unwrap_or_else(|| "—".into())
}

pub mod view {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct OwnBallot {
        pub choice: Choice,
        pub cast_at: String,
    }

    /// Server confirmation of an accepted (persisted) public ballot.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct CastConfirmation {
        pub proposal_id: String,
        pub choice: Choice,
        pub cast_at: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct VoterState {
        pub eligible: bool,
        pub ballot: Option<OwnBallot>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct VotePage {
        pub summary: ProposalSummary,
        /// In voting and before the deadline at the time of the read.
        pub open: bool,
        pub ends_at: Option<String>,
        pub electorate: Option<u64>,
        pub voting_enabled: bool,
        pub reauth_action: String,
        /// Synthetic/test instance (always a demonstration in this build).
        pub demonstration: bool,
        pub viewer: Option<VoterState>,
    }

    /// Own ballot and whether a client attempt key was recorded with a result.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct VoteStatus {
        pub ballot: Option<OwnBallot>,
        pub attempt_recorded: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ResultView {
        pub tally: Tally,
        pub electorate: u64,
        pub snapshot_id: String,
        pub setup_version_id: String,
        pub setup_version: Option<u32>,
        pub quorum_bp: u32,
        pub quorum_required: u64,
        pub majority_bp: u32,
        pub majority_required: u64,
        pub outcome: Outcome,
        pub closed_at: String,
        /// Setup kinds: package adopted (version) or not.
        pub package: Option<Option<u32>>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ResultState {
        /// Voting never opened (collection/debate) or the proposal left before it.
        NotOpened,
        /// No partial counts are published before closing.
        Open {
            ends_at: String,
            electorate: u64,
        },
        /// Deadline passed; closing is pending (ticker).
        Closing,
        Closed(ResultView),
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ResultPage {
        pub summary: ProposalSummary,
        pub state: ResultState,
        pub demonstration: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SetupOverview {
        pub effective_version: Option<u32>,
        pub effective_adopted_at: Option<String>,
        pub open_plebiscite: Option<ProposalSummary>,
        pub past_plebiscites: Vec<ProposalSummary>,
        pub suggested: crate::domain::setup::SetupParams,
        pub can_start: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SetupDiff {
        pub jurisdiction: String,
        pub current_version: Option<u32>,
        pub current: Option<crate::domain::setup::SetupParams>,
        pub proposed: crate::domain::setup::SetupParams,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ClosedVoteStat {
        pub id: String,
        pub title: String,
        pub status: crate::domain::proposals::ProposalStatus,
        pub electorate: u64,
        pub participation: u64,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct TerritoryCount {
        pub label: String,
        /// `None` when below [`MIN_CELL`] (suppressed).
        pub count: Option<u64>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Statistics {
        pub by_status: Vec<(crate::domain::proposals::ProposalStatus, u64)>,
        pub by_kind: Vec<(ProposalKind, u64)>,
        pub closed_votes: Vec<ClosedVoteStat>,
        pub citizens_by_state: Vec<TerritoryCount>,
        /// `None` when publishing it would reveal a suppressed cell.
        pub citizens_total: Option<u64>,
        pub min_cell: u64,
    }
}

/// Suppression rule for territorial counts: cells below `MIN_CELL` are hidden;
/// the total is hidden when the hidden cells add up to 1..MIN_CELL-1, because the
/// total minus the published cells would then reveal a small group.
pub fn suppress(counts: &[u64]) -> (Vec<Option<u64>>, Option<u64>) {
    let cells: Vec<Option<u64>> = counts
        .iter()
        .map(|&c| (c >= MIN_CELL).then_some(c))
        .collect();
    let hidden: u64 = counts.iter().filter(|&&c| c < MIN_CELL).sum();
    let total: u64 = counts.iter().sum();
    let total = (total >= MIN_CELL && (hidden == 0 || hidden >= MIN_CELL)).then_some(total);
    (cells, total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn choices_round_trip() {
        for c in Choice::ALL {
            assert_eq!(Choice::from_db(c.as_db()), Some(c));
        }
        assert_eq!(Choice::from_db("maybe"), None);
    }

    #[test]
    fn display_shares_round_half_up() {
        assert_eq!(share_bp(1, 3), Some(3333));
        assert_eq!(share_bp(2, 3), Some(6667));
        assert_eq!(share_bp(1, 8), Some(1250));
        assert_eq!(share_bp(1, 20_000), Some(1), "0.5 bp rounds up");
        assert_eq!(share_bp(0, 5), Some(0));
        assert_eq!(share_bp(5, 5), Some(10_000));
        assert_eq!(share_bp(1, 0), None);
        assert_eq!(format_bp(5234), "52,34%");
        assert_eq!(format_bp(5), "0,05%");
        assert_eq!(format_share(1, 0), "—");
    }

    #[test]
    fn small_cells_and_revealing_totals_are_suppressed() {
        assert_eq!(suppress(&[12, 30]), (vec![Some(12), Some(30)], Some(42)));
        assert_eq!(
            suppress(&[12, 3]),
            (vec![Some(12), None], None),
            "total would reveal 3"
        );
        assert_eq!(
            suppress(&[12, 4, 6]),
            (vec![Some(12), None, None], Some(22))
        );
        assert_eq!(suppress(&[9]), (vec![None], None));
        assert_eq!(suppress(&[]), (vec![], None));
    }

    proptest! {
        #[test]
        fn share_is_nearest_basis_point(part in 0u64..1_000_000, extra in 0u64..1_000_000) {
            let total = part + extra;
            prop_assume!(total > 0);
            let bp = u128::from(share_bp(part, total).unwrap());
            let exact2 = 2 * u128::from(part) * 10_000; // 2 × exact × total
            let t = u128::from(total);
            // |bp - exact| <= 1/2, ties up.
            prop_assert!(2 * bp * t <= exact2 + t);
            prop_assert!(2 * bp * t + t > exact2);
        }
    }
}
