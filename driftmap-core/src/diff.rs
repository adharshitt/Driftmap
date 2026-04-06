use std::collections::HashMap;
use crate::matcher::MatchedPair;

#[derive(Debug, Clone)]
pub struct RawProtocolDivergence {
    pub endpoint:        String,
    pub status_match:    bool,
    pub status_a:        u16,
    pub status_b:        u16,
    pub headers_only_a:  Vec<String>,
    pub headers_only_b:  Vec<String>,
    pub headers_value_diff: Vec<(String, String, String)>,
    pub body_identical:  bool,
    pub body_a_len:      usize,
    pub body_b_len:      usize,
    pub latency_delta_us: i64,
}

pub fn calculate_protocol_divergence(pair: &MatchedPair) -> RawProtocolDivergence {
    let status_match = pair.res_a.status == pair.res_b.status;

    let hdrs_a: HashMap<String, String> = pair.res_a.headers.iter().cloned().collect();
    let hdrs_b: HashMap<String, String> = pair.res_b.headers.iter().cloned().collect();

    let skip_headers = ["date", "x-request-id", "x-trace-id", "server-timing"];

    let mut headers_only_a = Vec::new();
    for k in hdrs_a.keys() {
        if !hdrs_b.contains_key(k) && !skip_headers.contains(&k.as_str()) {
            headers_only_a.push(k.clone());
        }
    }

    let mut headers_only_b = Vec::new();
    for k in hdrs_b.keys() {
        if !hdrs_a.contains_key(k) && !skip_headers.contains(&k.as_str()) {
            headers_only_b.push(k.clone());
        }
    }

    let mut headers_value_diff = Vec::new();
    for (k, va) in &hdrs_a {
        if let Some(vb) = hdrs_b.get(k) {
            if va != vb && !skip_headers.contains(&k.as_str()) {
                headers_value_diff.push((k.clone(), va.clone(), vb.clone()));
            }
        }
    }

    let body_identical = pair.res_a.body == pair.res_b.body;

    // Latency is the difference between response capture times
    // In a real scenario, this would be (res_capture - req_capture)
    let lat_a = pair.res_a.captured_at.duration_since(pair.req_a.captured_at).as_micros() as i64;
    let lat_b = pair.res_b.captured_at.duration_since(pair.req_b.captured_at).as_micros() as i64;

    RawProtocolDivergence {
        endpoint:        pair.endpoint.clone(),
        status_match,
        status_a:        pair.res_a.status,
        status_b:        pair.res_b.status,
        headers_only_a,
        headers_only_b,
        headers_value_diff,
        body_identical,
        body_a_len:      pair.res_a.body.len(),
        body_b_len:      pair.res_b.body.len(),
        latency_delta_us: lat_a - lat_b,
    }
}
