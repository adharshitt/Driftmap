use serde_json::Value;
use std::collections::HashSet;

pub struct SemanticNormalizer {
    pub ignore_fields: HashSet<String>,
}

impl SemanticNormalizer {
    pub fn new(ignore_fields: Vec<String>) -> Self {
        let mut set = HashSet::new();
        for f in ignore_fields {
            set.insert(f);
        }
        // Default ignored fields
        set.insert("id".to_string());
        set.insert("request_id".to_string());
        set.insert("trace_id".to_string());
        // Task 92: Sensitive fields to scrub
        set.insert("password".to_string());
        set.insert("secret".to_string());
        set.insert("token".to_string());
        set.insert("api_key".to_string());
        set.insert("authorization".to_string());
        Self { ignore_fields: set }
    }

    pub fn normalize(&self, body: &[u8]) -> Option<Vec<u8>> {
        let mut val: Value = serde_json::from_slice(body).ok()?;
        self.normalize_value(&mut val);
        serde_json::to_vec(&val).ok()
    }

    fn normalize_value(&self, value: &mut Value) {
        match value {
            Value::Object(map) => {
                // 1. Remove ignored fields
                map.retain(|k, v| {
                    if self.ignore_fields.contains(k) {
                        return false;
                    }

                    // Task 30: Auto-detect and ignore timestamps
                    if let Value::String(s) = v {
                        // Very simple ISO-8601 heuristic: YYYY-MM-DDTHH:MM:SS
                        if s.len() >= 19 && s.contains('-') && s.contains('T') && s.contains(':') {
                            return false;
                        }

                        // Task 38: Auto-detect and ignore UUIDs
                        // Simple heuristic: 8-4-4-4-12 pattern
                        if s.len() == 36 && s.chars().filter(|&c| c == '-').count() == 4 {
                            return false;
                        }
                    }
                    true
                });

                // 2. Recurse
                for v in map.values_mut() {
                    self.normalize_value(v);
                }
            }
            Value::Array(arr) => {
                for v in arr.iter_mut() {
                    self.normalize_value(v);
                }

                // Task 33: Sort arrays of primitives to ignore order drift
                // Only sort if all elements are primitives (strings, numbers, booleans)
                if arr
                    .iter()
                    .all(|v| v.is_string() || v.is_number() || v.is_boolean())
                {
                    arr.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
                }
            }
            _ => {}
        }
    }
}
