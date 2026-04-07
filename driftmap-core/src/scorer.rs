use std::collections::{HashMap, VecDeque};
use crate::diff::RawProtocolDivergence;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct SystemHealth {
    pub packets_per_sec: u64,
    pub buffer_utilization: f32,
    pub active_streams: usize,
    pub dropped_packets: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DashboardUpdate {
    pub scores: Vec<BehavioralDivergenceScore>,
    pub health: SystemHealth,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    pub schema_inferrer: crate::schema::SchemaInferrer,
    pub distribution: crate::distribution::FieldDistribution,

    pub normalizer: crate::semantic::SemanticNormalizer,
    recent_diffs: HashMap<String, VecDeque<RawProtocolDivergence>>,
    window_size:   usize,
}

impl Default for Scorer {
    fn default() -> Self {
        Self::new(vec![])
    }
}

impl Scorer {
    pub fn new(ignore_fields: Vec<String>) -> Self {
        Self {
            normalizer: crate::semantic::SemanticNormalizer::new(ignore_fields),
            schema_inferrer: crate::schema::SchemaInferrer::new(),
            distribution: crate::distribution::FieldDistribution::new(),
            recent_diffs: HashMap::new(),
            window_size:   100,
        }
    }

    pub fn score_pair(&mut self, _endpoint: &str, status_a: u16, status_b: u16, body_a: &[u8], body_b: &[u8]) -> f32 {
        let norm_a = self.normalizer.normalize(body_a).unwrap_or_else(|| body_a.to_vec());
        let norm_b = self.normalizer.normalize(body_b).unwrap_or_else(|| body_b.to_vec());

        let status_score: f32 = if status_a != status_b { 0.5 } else { 0.0 };
        let body_score: f32 = if norm_a != norm_b { 0.5 } else { 0.0 };

        (status_score + body_score).min(1.0)
    }

    pub fn ingest_diff(&mut self, diff: RawProtocolDivergence) {
        self.distribution.observe(crate::matcher::Target::A, diff.latency_delta_us as f64);
        self.distribution.observe(crate::matcher::Target::B, 0.0);

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
        
        let schema_score = if self.schema_inferrer.diff(endpoint).is_some() { 1.0 } else { 0.0 };
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
