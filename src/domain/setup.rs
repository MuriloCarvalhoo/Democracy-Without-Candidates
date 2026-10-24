//! Setup (electoral configuration) parameters. Values are agent proposals for
//! review (PRD §2.3), adopted only by plebiscite/meta-law, never by direct edit.

use serde::{Deserialize, Serialize};

use super::proposals::{Level, ProposalKind};

/// Basis points: 10000 = 100%.
pub const BP_FULL: u32 = 10_000;

/// Parameters of one setup version, stored as explicit integer columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupParams {
    pub signature_bp_national: u32,
    pub signature_bp_state: u32,
    pub signature_bp_municipal: u32,
    pub normal_quorum_bp: u32,
    pub normal_majority_bp: u32,
    pub normal_collect_days: u32,
    pub normal_debate_days: u32,
    pub normal_vote_days: u32,
    pub urgent_quorum_bp: u32,
    pub urgent_majority_bp: u32,
    pub urgent_collect_days: u32,
    pub urgent_vote_days: u32,
    pub setup_quorum_bp: u32,
    pub setup_majority_bp: u32,
    pub setup_collect_days: u32,
    pub setup_debate_days: u32,
    pub setup_vote_days: u32,
    pub initial_quorum_bp: u32,
    pub initial_majority_bp: u32,
    pub initial_vote_days: u32,
    pub quota_normal_per_year: u32,
    pub quota_urgent_per_year: u32,
}

/// Suggested package from PRD §2.3. Not adopted: only a plebiscite adopts it.
/// Simple majority (SIM > NÃO) is `5000`; setup majority (SIM > 60%) is `6000`.
pub const SUGGESTED: SetupParams = SetupParams {
    signature_bp_national: 50,
    signature_bp_state: 100,
    signature_bp_municipal: 200,
    normal_quorum_bp: 2500,
    normal_majority_bp: 5000,
    normal_collect_days: 90,
    normal_debate_days: 15,
    normal_vote_days: 30,
    urgent_quorum_bp: 2000,
    urgent_majority_bp: 5000,
    urgent_collect_days: 15,
    urgent_vote_days: 7,
    setup_quorum_bp: 3500,
    setup_majority_bp: 6000,
    setup_collect_days: 90,
    setup_debate_days: 15,
    setup_vote_days: 30,
    initial_quorum_bp: 4000,
    initial_majority_bp: 6000,
    initial_vote_days: 30,
    quota_normal_per_year: 5,
    quota_urgent_per_year: 1,
};

/// Durations and rules applicable to one proposal kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindRules {
    pub quorum_bp: u32,
    pub majority_bp: u32,
    pub collect_days: Option<u32>,
    pub debate_days: Option<u32>,
    pub vote_days: u32,
    pub quota_per_year: Option<u32>,
}

impl SetupParams {
    pub fn signature_bp(&self, level: Level) -> u32 {
        match level {
            Level::National => self.signature_bp_national,
            Level::State => self.signature_bp_state,
            Level::Municipal => self.signature_bp_municipal,
        }
    }

    pub fn rules(&self, kind: ProposalKind) -> KindRules {
        match kind {
            ProposalKind::NormalLaw => KindRules {
                quorum_bp: self.normal_quorum_bp,
                majority_bp: self.normal_majority_bp,
                collect_days: Some(self.normal_collect_days),
                debate_days: Some(self.normal_debate_days),
                vote_days: self.normal_vote_days,
                quota_per_year: Some(self.quota_normal_per_year),
            },
            ProposalKind::UrgentLaw => KindRules {
                quorum_bp: self.urgent_quorum_bp,
                majority_bp: self.urgent_majority_bp,
                collect_days: Some(self.urgent_collect_days),
                debate_days: None,
                vote_days: self.urgent_vote_days,
                quota_per_year: Some(self.quota_urgent_per_year),
            },
            ProposalKind::SetupChange => KindRules {
                quorum_bp: self.setup_quorum_bp,
                majority_bp: self.setup_majority_bp,
                collect_days: Some(self.setup_collect_days),
                debate_days: Some(self.setup_debate_days),
                vote_days: self.setup_vote_days,
                quota_per_year: None,
            },
            ProposalKind::InitialSetup => KindRules {
                quorum_bp: self.initial_quorum_bp,
                majority_bp: self.initial_majority_bp,
                collect_days: None,
                debate_days: None,
                vote_days: self.initial_vote_days,
                quota_per_year: None,
            },
        }
    }

    /// Ordered `(field, value)` pairs; also the column names in `setup_versions`.
    pub fn fields(&self) -> [(&'static str, u32); 22] {
        [
            ("signature_bp_national", self.signature_bp_national),
            ("signature_bp_state", self.signature_bp_state),
            ("signature_bp_municipal", self.signature_bp_municipal),
            ("normal_quorum_bp", self.normal_quorum_bp),
            ("normal_majority_bp", self.normal_majority_bp),
            ("normal_collect_days", self.normal_collect_days),
            ("normal_debate_days", self.normal_debate_days),
            ("normal_vote_days", self.normal_vote_days),
            ("urgent_quorum_bp", self.urgent_quorum_bp),
            ("urgent_majority_bp", self.urgent_majority_bp),
            ("urgent_collect_days", self.urgent_collect_days),
            ("urgent_vote_days", self.urgent_vote_days),
            ("setup_quorum_bp", self.setup_quorum_bp),
            ("setup_majority_bp", self.setup_majority_bp),
            ("setup_collect_days", self.setup_collect_days),
            ("setup_debate_days", self.setup_debate_days),
            ("setup_vote_days", self.setup_vote_days),
            ("initial_quorum_bp", self.initial_quorum_bp),
            ("initial_majority_bp", self.initial_majority_bp),
            ("initial_vote_days", self.initial_vote_days),
            ("quota_normal_per_year", self.quota_normal_per_year),
            ("quota_urgent_per_year", self.quota_urgent_per_year),
        ]
    }

    /// Builds parameters from values in `fields()` order.
    pub fn from_values(v: [u32; 22]) -> Self {
        Self {
            signature_bp_national: v[0],
            signature_bp_state: v[1],
            signature_bp_municipal: v[2],
            normal_quorum_bp: v[3],
            normal_majority_bp: v[4],
            normal_collect_days: v[5],
            normal_debate_days: v[6],
            normal_vote_days: v[7],
            urgent_quorum_bp: v[8],
            urgent_majority_bp: v[9],
            urgent_collect_days: v[10],
            urgent_vote_days: v[11],
            setup_quorum_bp: v[12],
            setup_majority_bp: v[13],
            setup_collect_days: v[14],
            setup_debate_days: v[15],
            setup_vote_days: v[16],
            initial_quorum_bp: v[17],
            initial_majority_bp: v[18],
            initial_vote_days: v[19],
            quota_normal_per_year: v[20],
            quota_urgent_per_year: v[21],
        }
    }

    /// Range check per field; returns the first invalid field name.
    pub fn validate(&self) -> Result<(), &'static str> {
        for (name, value) in self.fields() {
            let range = if name.contains("majority") {
                5000..=9999
            } else if name.contains("_bp") {
                1..=BP_FULL
            } else if name.contains("collect") {
                1..=365
            } else if name.contains("debate") {
                1..=180
            } else if name.contains("vote_days") {
                1..=90
            } else {
                1..=100
            };
            if !range.contains(&value) {
                return Err(name);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_package_is_valid_and_round_trips() {
        assert_eq!(SUGGESTED.validate(), Ok(()));
        let values = SUGGESTED.fields().map(|(_, v)| v);
        assert_eq!(SetupParams::from_values(values), SUGGESTED);
    }

    #[test]
    fn rejects_out_of_range_fields() {
        let mut p = SUGGESTED;
        p.setup_majority_bp = 4999;
        assert_eq!(p.validate(), Err("setup_majority_bp"));
        let mut p = SUGGESTED;
        p.signature_bp_state = 0;
        assert_eq!(p.validate(), Err("signature_bp_state"));
        let mut p = SUGGESTED;
        p.urgent_vote_days = 91;
        assert_eq!(p.validate(), Err("urgent_vote_days"));
    }

    #[test]
    fn kind_rules_follow_flows() {
        let urgent = SUGGESTED.rules(ProposalKind::UrgentLaw);
        assert_eq!(urgent.debate_days, None);
        assert_eq!(urgent.quota_per_year, Some(1));
        let meta = SUGGESTED.rules(ProposalKind::SetupChange);
        assert_eq!((meta.vote_days, meta.majority_bp), (30, 6000));
        assert_eq!(
            SUGGESTED.rules(ProposalKind::InitialSetup).collect_days,
            None
        );
    }
}
