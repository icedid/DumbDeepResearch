use ddr_protocol::SuggestedBranch;

/// Policy controller governing Free Exploration (Auto-Pilot) vs. Human-in-the-Loop Steering.
#[derive(Debug, Clone, Copy)]
pub struct SteeringPolicy {
    pub max_depth: u8,
    pub novelty_threshold: f32,
}

impl SteeringPolicy {
    pub fn new(max_depth: u8, novelty_threshold: f32) -> Self {
        Self {
            max_depth,
            novelty_threshold,
        }
    }

    /// In Autonomous Free Exploration mode, evaluates which `Proposed` child branches
    /// have sufficient marginal information gain to auto-trigger without human intervention.
    pub fn select_auto_branches<'a>(
        &self,
        current_depth: u8,
        proposed: &'a [SuggestedBranch],
    ) -> Vec<&'a SuggestedBranch> {
        if current_depth >= self.max_depth {
            return Vec::new();
        }

        let mut selected: Vec<&'a SuggestedBranch> = proposed
            .iter()
            .filter(|branch| branch.significance_score >= self.novelty_threshold)
            .collect();

        selected.sort_by(|a, b| b.significance_score.total_cmp(&a.significance_score));
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddr_protocol::FacetSlot;

    #[test]
    fn respects_max_depth_and_threshold() {
        let policy = SteeringPolicy::new(2, 0.6);
        let branches = vec![
            SuggestedBranch {
                topic: "Low".into(),
                query: "low query".into(),
                slot: FacetSlot::Exploratory,
                significance_score: 0.4,
            },
            SuggestedBranch {
                topic: "Mid".into(),
                query: "mid query".into(),
                slot: FacetSlot::Economic,
                significance_score: 0.65,
            },
            SuggestedBranch {
                topic: "High".into(),
                query: "high query".into(),
                slot: FacetSlot::Quantitative,
                significance_score: 0.9,
            },
        ];

        assert!(policy.select_auto_branches(2, &branches).is_empty());

        let chosen = policy.select_auto_branches(0, &branches);
        assert_eq!(chosen.len(), 2);
        assert_eq!(chosen[0].topic, "High");
        assert_eq!(chosen[1].topic, "Mid");
    }
}
