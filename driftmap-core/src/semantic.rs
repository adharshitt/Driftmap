use serde_json::{Value, Map};
use std::collections::HashSet;

pub struct SemanticNormalizer {
    pub ignore_fields: HashSet<String>,
}

impl SemanticNormalizer {
    pub fn new(ignore_fields: Vec<String>) -> Self {
        let mut set = HashSet::new();
        for f in ignore_fields { set.insert(f); }
        // Default ignored fields
        set.insert("id".to_string());
        set.insert("request_id".to_string());
        set.insert("trace_id".to_string());
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
                map.retain(|k, _| !self.ignore_fields.contains(k));
                
                // 2. Recurse
                for v in map.values_mut() {
                    self.normalize_value(v);
                }

                // 3. Sorting is handled by serde_json::Map (it's a BTreeMap internally)
            }
            Value::Array(arr) => {
                for v in arr {
                    self.normalize_value(v);
                }
            }
            _ => {}
        }
    }
}
