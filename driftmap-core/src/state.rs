use crate::scorer::BehavioralDivergenceScore;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum DriftState {
    Unknown,
    Equivalent,
    Drifting,
    Diverged,
}

pub struct StateRecord {
    pub state: DriftState,
    pub entered_at: Instant,
    pub threshold_crossed_at: Option<Instant>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StateTransition {
    pub endpoint: String,
    pub from: DriftState,
    pub to: DriftState,
}

pub struct StateMachine {
    records: HashMap<String, StateRecord>,
    hysteresis: Duration,
    drift_threshold: f32,
    diverged_threshold: f32,
    min_samples: u64,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            hysteresis: Duration::from_secs(30),
            drift_threshold: 0.05,
            diverged_threshold: 0.50,
            min_samples: 50,
        }
    }

    pub fn update(
        &mut self,
        endpoint: &str,
        score: &BehavioralDivergenceScore,
    ) -> Option<StateTransition> {
        if score.sample_count < self.min_samples {
            self.records
                .entry(endpoint.to_string())
                .or_insert_with(|| StateRecord {
                    state: DriftState::Unknown,
                    entered_at: Instant::now(),
                    threshold_crossed_at: None,
                });
            return None;
        }

        let record = self
            .records
            .entry(endpoint.to_string())
            .or_insert_with(|| StateRecord {
                state: DriftState::Equivalent,
                entered_at: Instant::now(),
                threshold_crossed_at: None,
            });

        let target_state = if score.score >= self.diverged_threshold {
            DriftState::Diverged
        } else if score.score >= self.drift_threshold {
            DriftState::Drifting
        } else {
            DriftState::Equivalent
        };

        if target_state == record.state {
            record.threshold_crossed_at = None;
            return None;
        }

        match record.threshold_crossed_at {
            None => {
                record.threshold_crossed_at = Some(Instant::now());
                None
            }
            Some(crossed_at) => {
                if crossed_at.elapsed() >= self.hysteresis {
                    let old = record.state.clone();
                    record.state = target_state.clone();
                    record.entered_at = Instant::now();
                    record.threshold_crossed_at = None;
                    Some(StateTransition {
                        endpoint: endpoint.to_string(),
                        from: old,
                        to: target_state,
                    })
                } else {
                    None
                }
            }
        }
    }
}
