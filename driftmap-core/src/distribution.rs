use crate::matcher::Target;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Centroid {
    mean:  f64,
    count: u32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StreamingQuantileEstimator {
    centroids: Vec<Centroid>,
    count:     u64,
    max_size:  usize,
}

impl Default for StreamingQuantileEstimator {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingQuantileEstimator {
    pub fn new() -> Self {
        Self { centroids: Vec::new(), count: 0, max_size: 200 }
    }

    pub fn add(&mut self, value: f64) {
        self.count += 1;
        let pos = self.centroids.partition_point(|c| c.mean < value);
        self.centroids.insert(pos, Centroid { mean: value, count: 1 });
        if self.centroids.len() > self.max_size * 2 {
            self.compress();
        }
    }

    pub fn quantile(&self, q: f64) -> f64 {
        if self.centroids.is_empty() { return 0.0; }
        let target = q * self.count as f64;
        let mut seen = 0.0_f64;
        for c in &self.centroids {
            seen += c.count as f64;
            if seen >= target { return c.mean; }
        }
        self.centroids.last().unwrap().mean
    }

    fn compress(&mut self) {
        let mut merged: Vec<Centroid> = Vec::with_capacity(self.max_size);
        let total = self.count as f64;

        // Sort by mean before merging
        self.centroids.sort_by(|a, b| a.mean.partial_cmp(&b.mean).unwrap());

        for c in self.centroids.drain(..) {
            let merged_len = merged.len();
            if let Some(last) = merged.last_mut() {
                let combined_count = last.count + c.count;
                // t-digest scaling function for bounded size
                let k = merged_len as f64 / self.max_size as f64;
                let limit = 4.0 * total * k * (1.0 - k);
                
                if combined_count as f64 <= limit {
                    last.mean = (last.mean * last.count as f64 + c.mean * c.count as f64)
                        / combined_count as f64;
                    last.count = combined_count;
                    continue;
                }
            }
            merged.push(c);
        }
        self.centroids = merged;
    }
}

pub struct FieldDistribution {
    pub digest_a: StreamingQuantileEstimator,
    pub digest_b: StreamingQuantileEstimator,
}

impl Default for FieldDistribution {
    fn default() -> Self {
        Self::new()
    }
}

impl FieldDistribution {
    pub fn new() -> Self {
        Self {
            digest_a: StreamingQuantileEstimator::new(),
            digest_b: StreamingQuantileEstimator::new(),
        }
    }

    pub fn observe(&mut self, target: Target, value: f64) {
        match target {
            Target::A => self.digest_a.add(value),
            Target::B => self.digest_b.add(value),
        }
    }

    pub fn divergence_score(&self) -> f32 {
        if self.digest_a.count < 10 || self.digest_b.count < 10 { return 0.0; }

        let p95_a = self.digest_a.quantile(0.95);
        let p95_b = self.digest_b.quantile(0.95);
        let p50_a = self.digest_a.quantile(0.50);
        let p50_b = self.digest_b.quantile(0.50);

        let p95_diff = ((p95_a - p95_b).abs() / (p95_a.max(p95_b) + 1.0)) as f32;
        let p50_diff = ((p50_a - p50_b).abs() / (p50_a.max(p50_b) + 1.0)) as f32;

        (p95_diff * 0.7 + p50_diff * 0.3).min(1.0)
    }
}
