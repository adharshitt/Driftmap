pub mod capture;
pub mod diff;
pub mod distribution;
pub mod export;
pub mod http;
pub mod matcher;
pub mod pipeline;
pub mod plugins;
pub mod schema;
pub mod scorer;
pub mod semantic;
pub mod state;
pub mod store;

// Re-export high-level types for SDK usage
pub use matcher::{MatchedPair, Target};
pub use scorer::{Scorer, BehavioralDivergenceScore, DashboardUpdate};
pub use pipeline::initialize_observability_pipeline;

/// A high-level session for programmatic behavioral diffing
pub struct DriftSession {
    scorer: Scorer,
}

impl DriftSession {
    pub fn new(ignore_fields: Vec<String>) -> Self {
        Self {
            scorer: Scorer::new(ignore_fields),
        }
    }

    pub fn score(&mut self, endpoint: &str, status_a: u16, status_b: u16, body_a: &[u8], body_b: &[u8]) -> f32 {
        self.scorer.score_pair(endpoint, status_a, status_b, body_a, body_b)
    }
}
