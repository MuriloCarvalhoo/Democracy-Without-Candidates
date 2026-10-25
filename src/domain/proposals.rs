//! Proposal lifecycle and electoral arithmetic. Pure and exact (integer basis
//! points); rules are agent proposals for review (SDD "Regras eleitorais").

use serde::{Deserialize, Serialize};

use super::setup::BP_FULL;

macro_rules! db_enum {
    ($name:ident { $($variant:ident => $db:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub fn as_db(self) -> &'static str {
                match self { $(Self::$variant => $db),+ }
            }

            pub fn from_db(value: &str) -> Option<Self> {
                match value { $($db => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

db_enum!(ProposalKind {
    NormalLaw => "normal_law",
    UrgentLaw => "urgent_law",
    SetupChange => "setup_change",
    InitialSetup => "initial_setup",
});

db_enum!(ProposalStatus {
    Draft => "draft",
    CollectingSignatures => "collecting_signatures",
    InDebate => "in_debate",
    InVoting => "in_voting",
    Approved => "approved",
    Rejected => "rejected",
    QuorumNotReached => "quorum_not_reached",
    Expired => "expired",
    Archived => "archived",
});

db_enum!(Level {
    National => "national",
    State => "state",
    Municipal => "municipal",
});

impl ProposalKind {
    /// Kinds a citizen may draft. `initial_setup` is started by the country admin.
    pub fn citizen_creatable(self) -> bool {
        !matches!(self, Self::InitialSetup)
    }

    /// Phase entered when the signature threshold is reached.
    pub fn after_collection(self) -> Option<ProposalStatus> {
        match self {
            Self::NormalLaw | Self::SetupChange => Some(ProposalStatus::InDebate),
            Self::UrgentLaw => Some(ProposalStatus::InVoting),
            Self::InitialSetup => None,
        }
    }
}

impl ProposalKind {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::NormalLaw => "proposals.kind.normal_law",
            Self::UrgentLaw => "proposals.kind.urgent_law",
            Self::SetupChange => "proposals.kind.setup_change",
            Self::InitialSetup => "proposals.kind.initial_setup",
        }
    }
}

impl Level {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::National => "proposals.level.national",
            Self::State => "proposals.level.state",
            Self::Municipal => "proposals.level.municipal",
        }
    }
}

impl ProposalStatus {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Draft => "proposals.status.draft",
            Self::CollectingSignatures => "proposals.status.collecting_signatures",
            Self::InDebate => "proposals.status.in_debate",
            Self::InVoting => "proposals.status.in_voting",
            Self::Approved => "proposals.status.approved",
            Self::Rejected => "proposals.status.rejected",
            Self::QuorumNotReached => "proposals.status.quorum_not_reached",
            Self::Expired => "proposals.status.expired",
            Self::Archived => "proposals.status.archived",
        }
    }

    /// Final outcome of a phase; only archiving may follow.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Approved | Self::Rejected | Self::QuorumNotReached | Self::Expired
        )
    }

    pub fn is_closed(self) -> bool {
        self.is_terminal() || self == Self::Archived
    }
}

/// Allowed lifecycle transitions per kind. `Draft → Archived` and
/// `CollectingSignatures → Archived` are author withdrawals; terminal → Archived is
/// administrative archiving. `InitialSetup` starts directly in voting.
pub fn can_transition(kind: ProposalKind, from: ProposalStatus, to: ProposalStatus) -> bool {
    use ProposalStatus::*;
    match (from, to) {
        (Draft, CollectingSignatures) => kind.citizen_creatable(),
        (Draft | CollectingSignatures, Archived) => kind.citizen_creatable(),
        (CollectingSignatures, Expired) => true,
        (CollectingSignatures, next) => kind.after_collection() == Some(next),
        (InDebate, InVoting) => matches!(kind, ProposalKind::NormalLaw | ProposalKind::SetupChange),
        (InVoting, Approved | Rejected | QuorumNotReached) => true,
        (from, Archived) => from.is_terminal(),
        _ => false,
    }
}

/// Minimum count for a basis-point share of an electorate: ceil(bp × n / 10000).
pub fn min_count(bp: u32, electorate: u64) -> u64 {
    let n = u128::from(bp) * u128::from(electorate);
    n.div_ceil(u128::from(BP_FULL)) as u64
}

/// SIM votes required for a majority over valid votes: floor(bp × valid / 10000) + 1.
/// `5000` is equivalent to SIM > NÃO; `6000` to SIM > 60% of valid votes.
pub fn majority_required(majority_bp: u32, valid: u64) -> u64 {
    (u128::from(majority_bp) * u128::from(valid) / u128::from(BP_FULL)) as u64 + 1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Tally {
    pub yes: u64,
    pub no: u64,
    pub abstain: u64,
}

impl Tally {
    pub fn participation(&self) -> u64 {
        self.yes + self.no + self.abstain
    }

    pub fn valid(&self) -> u64 {
        self.yes + self.no
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    NoValidVotes,
    Tie,
    MajorityNotReached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Approved,
    Rejected(RejectReason),
    QuorumNotReached,
}

impl Outcome {
    pub fn status(self) -> ProposalStatus {
        match self {
            Self::Approved => ProposalStatus::Approved,
            Self::Rejected(_) => ProposalStatus::Rejected,
            Self::QuorumNotReached => ProposalStatus::QuorumNotReached,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultCalc {
    pub quorum_required: u64,
    pub majority_required: u64,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ElectoralError {
    #[error("empty electorate cannot open an election")]
    EmptyElectorate,
    #[error("tally exceeds the electorate snapshot")]
    TallyExceedsElectorate,
}

/// Result of a closed vote over a materialized electorate snapshot.
/// Quorum counts SIM+NÃO+ABSTENÇÃO; majority uses valid votes (SIM+NÃO) only.
pub fn compute_result(
    tally: Tally,
    electorate: u64,
    quorum_bp: u32,
    majority_bp: u32,
) -> Result<ResultCalc, ElectoralError> {
    if electorate == 0 {
        return Err(ElectoralError::EmptyElectorate);
    }
    if tally.participation() > electorate {
        return Err(ElectoralError::TallyExceedsElectorate);
    }
    let quorum_required = min_count(quorum_bp, electorate);
    let majority_required = majority_required(majority_bp, tally.valid());
    let outcome = if tally.participation() < quorum_required {
        Outcome::QuorumNotReached
    } else if tally.valid() == 0 {
        Outcome::Rejected(RejectReason::NoValidVotes)
    } else if tally.yes == tally.no {
        Outcome::Rejected(RejectReason::Tie)
    } else if tally.yes >= majority_required {
        Outcome::Approved
    } else {
        Outcome::Rejected(RejectReason::MajorityNotReached)
    };
    Ok(ResultCalc {
        quorum_required,
        majority_required,
        outcome,
    })
}

/// Reauthentication actions (bound to the proposal id as resource).
pub const REAUTH_PUBLISH: &str = "proposal:create";
pub const REAUTH_SIGN: &str = "proposal:sign";

pub const TITLE_MAX: usize = 160;
pub const SUMMARY_MAX: usize = 600;
pub const BODY_MAX: usize = 20_000;
pub const REASON_MAX: usize = 500;

/// Normalized text for the duplicate warning: lowercase alphanumeric words,
/// accents kept, punctuation and repeated whitespace removed.
pub fn normalize_text(raw: &str) -> String {
    raw.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Shared view models returned by server functions (plain data).
pub mod view {
    use serde::{Deserialize, Serialize};

    use super::{Level, ProposalKind, ProposalStatus};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ProposalSummary {
        pub id: String,
        pub kind: ProposalKind,
        pub status: ProposalStatus,
        pub level: Level,
        pub jurisdiction: String,
        pub title: String,
        pub summary: String,
        pub signature_count: u64,
        pub signature_target: Option<u64>,
        /// Next deadline of the current phase, formatted in the instance timezone.
        pub deadline: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ProposalPage {
        pub items: Vec<ProposalSummary>,
        pub page: u32,
        pub pages: u32,
        pub total: u64,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Deadline {
        /// Phase the deadline closes.
        pub phase: ProposalStatus,
        pub at: String,
        pub passed: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ViewerState {
        pub is_author: bool,
        pub eligible: bool,
        pub has_signed: bool,
        pub can_sign: bool,
        pub can_withdraw_signature: bool,
        pub can_edit: bool,
        pub can_publish: bool,
        pub can_withdraw: bool,
        pub can_archive: bool,
    }

    impl ViewerState {
        /// Whether the action card has anything to show (else it is not rendered).
        pub fn has_actions(&self) -> bool {
            self.has_signed
                || self.can_sign
                || self.can_withdraw_signature
                || self.can_edit
                || self.can_publish
                || self.can_withdraw
                || self.can_archive
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ProposalDetail {
        pub summary: ProposalSummary,
        pub body: String,
        pub text_hash: Option<String>,
        pub author: String,
        pub published_at: Option<String>,
        pub deadlines: Vec<Deadline>,
        pub setup_version: Option<u32>,
        pub archive_reason: Option<String>,
        pub proposed_setup: Option<crate::domain::setup::SetupParams>,
        pub viewer: Option<ViewerState>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Draft {
        pub id: String,
        pub kind: ProposalKind,
        pub level: Level,
        pub title: String,
        pub summary: String,
        pub body: String,
        pub proposed_setup: Option<crate::domain::setup::SetupParams>,
    }

    /// Draft fields sent by the author. Jurisdiction is derived on the server from
    /// the chosen level and the author's immutable territory.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct DraftInput {
        pub kind: ProposalKind,
        pub level: Level,
        pub title: String,
        pub summary: String,
        pub body: String,
        pub proposed_setup: Option<crate::domain::setup::SetupParams>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct DuplicateHit {
        pub id: String,
        pub title: String,
        pub status: ProposalStatus,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct DraftSaved {
        pub id: String,
        pub duplicates: Vec<DuplicateHit>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SignatureState {
        pub status: ProposalStatus,
        pub signature_count: u64,
        pub signature_target: u64,
        pub signed: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Published {
        pub id: String,
        pub status: ProposalStatus,
        pub signature_target: u64,
        pub collect_ends_at: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Signers {
        pub count: u64,
        /// Public signer names. Empty until profile visibility exists (identity).
        pub public_names: Vec<String>,
        pub visibility_available: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Dashboard {
        pub voting: Vec<ProposalSummary>,
        pub debate: Vec<ProposalSummary>,
        pub collecting: Vec<ProposalSummary>,
        pub mine: Option<Vec<ProposalSummary>>,
        pub signed: Option<Vec<ProposalSummary>>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SetupVersionView {
        pub id: String,
        pub jurisdiction: String,
        pub version: Option<u32>,
        pub status: String,
        pub synthetic: bool,
        pub adopted_at: Option<String>,
        pub source_proposal_id: Option<String>,
        pub params: crate::domain::setup::SetupParams,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ProposalStatus::*;
    use proptest::prelude::*;

    #[test]
    fn viewer_has_actions_only_when_something_is_allowed_or_signed() {
        let none = view::ViewerState {
            is_author: false,
            eligible: false,
            has_signed: false,
            can_sign: false,
            can_withdraw_signature: false,
            can_edit: false,
            can_publish: false,
            can_withdraw: false,
            can_archive: false,
        };
        assert!(!none.has_actions());
        // Being the author or eligible alone shows nothing to act on.
        let author = view::ViewerState {
            is_author: true,
            eligible: true,
            ..none.clone()
        };
        assert!(!author.has_actions());
        let mut each = Vec::new();
        for set in [
            |v: &mut view::ViewerState| v.has_signed = true,
            |v: &mut view::ViewerState| v.can_sign = true,
            |v: &mut view::ViewerState| v.can_withdraw_signature = true,
            |v: &mut view::ViewerState| v.can_edit = true,
            |v: &mut view::ViewerState| v.can_publish = true,
            |v: &mut view::ViewerState| v.can_withdraw = true,
            |v: &mut view::ViewerState| v.can_archive = true,
        ] {
            let mut v = none.clone();
            set(&mut v);
            each.push(v.has_actions());
        }
        assert!(each.into_iter().all(|shown| shown));
    }

    #[test]
    fn enums_round_trip_db_values() {
        for k in ProposalKind::ALL {
            assert_eq!(ProposalKind::from_db(k.as_db()), Some(*k));
        }
        for s in ProposalStatus::ALL {
            assert_eq!(ProposalStatus::from_db(s.as_db()), Some(*s));
        }
        assert_eq!(ProposalStatus::from_db("deleted"), None);
    }

    #[test]
    fn flows_per_kind() {
        use ProposalKind::*;
        assert!(can_transition(NormalLaw, CollectingSignatures, InDebate));
        assert!(!can_transition(NormalLaw, CollectingSignatures, InVoting));
        assert!(can_transition(UrgentLaw, CollectingSignatures, InVoting));
        assert!(!can_transition(UrgentLaw, CollectingSignatures, InDebate));
        assert!(!can_transition(UrgentLaw, InDebate, InVoting));
        assert!(can_transition(SetupChange, InDebate, InVoting));
        assert!(!can_transition(InitialSetup, Draft, CollectingSignatures));
        assert!(can_transition(InitialSetup, InVoting, Approved));
        assert!(can_transition(NormalLaw, CollectingSignatures, Archived));
        assert!(
            !can_transition(NormalLaw, InDebate, Archived),
            "no withdrawal after collection"
        );
        assert!(!can_transition(NormalLaw, InVoting, Archived));
        assert!(can_transition(NormalLaw, Expired, Archived));
        assert!(!can_transition(NormalLaw, Archived, Draft));
        assert!(!can_transition(NormalLaw, Approved, Rejected));
    }

    #[test]
    fn closed_states_have_no_exit_but_archive() {
        for kind in ProposalKind::ALL {
            for from in ProposalStatus::ALL.iter().filter(|s| s.is_closed()) {
                for to in ProposalStatus::ALL {
                    if can_transition(*kind, *from, *to) {
                        assert_eq!(*to, Archived, "{kind:?} {from:?} -> {to:?}");
                        assert!(from.is_terminal());
                    }
                }
            }
        }
    }

    #[test]
    fn arithmetic_boundaries() {
        assert_eq!(min_count(50, 1), 1, "any positive share needs one");
        assert_eq!(min_count(50, 200), 1);
        assert_eq!(min_count(50, 201), 2);
        assert_eq!(min_count(2500, 4), 1);
        assert_eq!(min_count(2500, 5), 2);
        assert_eq!(min_count(10_000, 7), 7);
        assert_eq!(min_count(1, 0), 0);
        assert_eq!(majority_required(6000, 10), 7, "> 60% of 10");
        assert_eq!(
            majority_required(6000, 5),
            4,
            "> 60% of 5 (3 = exactly 60%)"
        );
        assert_eq!(majority_required(5000, 4), 3);
        assert_eq!(majority_required(5000, 3), 2);
        assert_eq!(min_count(10_000, u64::MAX), u64::MAX, "no overflow");
    }

    #[test]
    fn result_cases() {
        let t = |yes, no, abstain| Tally { yes, no, abstain };
        let r = |tally, n| compute_result(tally, n, 2500, 5000).map(|c| c.outcome);
        assert_eq!(r(t(0, 0, 0), 0), Err(ElectoralError::EmptyElectorate));
        assert_eq!(
            r(t(3, 0, 0), 2),
            Err(ElectoralError::TallyExceedsElectorate)
        );
        assert_eq!(
            r(t(2, 0, 0), 8),
            Ok(Outcome::Approved),
            "quorum ceil(2.0) = 2"
        );
        assert_eq!(
            r(t(2, 0, 0), 9),
            Ok(Outcome::QuorumNotReached),
            "quorum ceil(2.25) = 3"
        );
        assert_eq!(
            r(t(0, 0, 3), 9),
            Ok(Outcome::Rejected(RejectReason::NoValidVotes))
        );
        assert_eq!(r(t(2, 2, 0), 9), Ok(Outcome::Rejected(RejectReason::Tie)));
        assert_eq!(
            r(t(2, 1, 5), 9),
            Ok(Outcome::Approved),
            "abstentions count for quorum only"
        );
        let s = |tally| compute_result(tally, 100, 3500, 6000).map(|c| c.outcome);
        assert_eq!(
            s(t(30, 20, 0)),
            Ok(Outcome::Rejected(RejectReason::MajorityNotReached)),
            "60% exactly"
        );
        assert_eq!(s(t(31, 20, 0)), Ok(Outcome::Approved));
    }

    #[test]
    fn normalization_ignores_case_punctuation_and_spacing() {
        assert_eq!(normalize_text("  Praça  Pública, já!"), "praça pública já");
        assert_eq!(
            normalize_text("PRAÇA pública já"),
            normalize_text("praça-pública já.")
        );
    }

    proptest! {
        #[test]
        fn min_count_is_the_exact_ceiling(bp in 1u32..=10_000, n in 0u64..10_000_000) {
            let m = min_count(bp, n);
            // m/n >= bp/10000 and (m-1)/n < bp/10000, in integers.
            prop_assert!(u128::from(m) * 10_000 >= u128::from(bp) * u128::from(n));
            if m > 0 {
                prop_assert!(u128::from(m - 1) * 10_000 < u128::from(bp) * u128::from(n));
            }
        }

        #[test]
        fn majority_is_strictly_greater_than_share(bp in 5000u32..=9999, valid in 0u64..1_000_000) {
            let m = majority_required(bp, valid);
            prop_assert!(u128::from(m) * 10_000 > u128::from(bp) * u128::from(valid));
            prop_assert!(u128::from(m - 1) * 10_000 <= u128::from(bp) * u128::from(valid));
        }

        #[test]
        fn simple_majority_equals_yes_greater_than_no(yes in 0u64..1000, no in 0u64..1000, abstain in 0u64..1000) {
            let tally = Tally { yes, no, abstain };
            let n = tally.participation().max(1);
            let outcome = compute_result(tally, n, 1, 5000).unwrap().outcome;
            prop_assert_eq!(outcome == Outcome::Approved, yes > no);
        }

        #[test]
        fn approval_requires_quorum(yes in 0u64..500, no in 0u64..500, abstain in 0u64..500, extra in 0u64..2000, q in 1u32..=10_000) {
            let tally = Tally { yes, no, abstain };
            let n = tally.participation() + extra;
            prop_assume!(n > 0);
            let calc = compute_result(tally, n, q, 6000).unwrap();
            if calc.outcome == Outcome::Approved {
                prop_assert!(tally.participation() >= calc.quorum_required);
                prop_assert!(yes > no && yes >= calc.majority_required);
            }
        }
    }
}
