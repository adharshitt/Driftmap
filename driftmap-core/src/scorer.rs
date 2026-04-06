use std::collections::{HashMap, VecDeque};
use crate::diff::RawProtocolDivergence;

#[derive(Debug, Clone, serde::Serialize)]
pub struct BehavioralDivergenceScore {
    pub endpoint:      String,
    pub score:         f32,
    pub status_score:  f32,
    pub schema_score:  f32,
    pub latency_score: f32,
    pub header_score:  f32,
    pub sample_count:  u64,
}

pub struct Scorer {
    pub normalizer: crate::semantic::SemanticNormalizer,
    recent_diffs: HashMap<String, VecDeque<RawProtocolDivergence>>,
    window_size:   usize,
}

impl Scorer {
    pub fn new() -> Self {
        Self {
            normalizer: crate::semantic::SemanticNormalizer::new(vec![]),
            recent_diffs: HashMap::new(),
            window_size: 1000,
        }
    }

    pub fn ingest_diff(&mut self, diff: RawProtocolDivergence) {
        let diffs = self.recent_diffs.entry(diff.endpoint.clone()).or_insert_with(|| VecDeque::with_capacity(1000));
        diffs.push_back(diff);
        if diffs.len() > self.window_size {
            diffs.pop_front();
        }
    }

    pub fn compute_score(&self, endpoint: &str) -> Option<BehavioralDivergenceScore> {
        let diffs = self.recent_diffs.get(endpoint)?;
        if diffs.is_empty() { return None; }

        let count = diffs.len() as f32;
        let status_score = diffs.iter().filter(|d| !d.status_match).count() as f32 / count;
        
        // Phase 2: Schema and Latency scores will be integrated here. 
        // For Phase 1 MVP, we use status and headers.
        let schema_score = 0.0;
        let latency_score = (diffs.iter().map(|d| d.latency_delta_us.abs()).sum::<i64>() as f32 / count / 100000.0).min(1.0);

        let header_score = diffs.iter().map(|d| {
            let total = d.headers_only_a.len() + d.headers_only_b.len() + d.headers_value_diff.len();
            (total as f32 / 10.0).min(1.0)
        }).sum::<f32>() / count;

        let score = (status_score * 0.40 + schema_score * 0.30 + latency_score * 0.20 + header_score * 0.10).clamp(0.0, 1.0);

        Some(BehavioralDivergenceScore {
            endpoint: endpoint.to_string(),
            score,
            status_score,
            schema_score,
            latency_score,
            header_score,
            sample_count: diffs.len() as u64,
        })
    }

    pub fn all_scores(&self) -> Vec<BehavioralDivergenceScore> {
        self.recent_diffs.keys().filter_map(|e| self.compute_score(e)).collect()
    }
}
