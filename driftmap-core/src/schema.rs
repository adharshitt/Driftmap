use crate::matcher::Target;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum FieldType {
    String,
    Integer,
    Float,
    Boolean,
    Object(SchemaNode),
    Array(Box<FieldType>),
    Null,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct FieldStats {
    pub field_type: FieldType,
    pub seen_count: u32,
    pub total_count: u32,
    pub nullable: bool,
}

impl FieldStats {
    pub fn presence_rate(&self) -> f32 {
        self.seen_count as f32 / self.total_count as f32
    }
    pub fn is_required(&self) -> bool {
        self.presence_rate() > 0.95
    }
}

pub type SchemaNode = HashMap<String, FieldStats>;

#[derive(Debug, Clone, serde::Serialize)]
pub struct StructuralSchemaDivergence {
    pub endpoint: String,
    pub fields_only_a: Vec<String>,
    pub fields_only_b: Vec<String>,
    pub type_mismatches: Vec<(String, FieldType, FieldType)>,
}

pub struct SchemaInferrer {
    pub schemas: HashMap<(String, Target), SchemaNode>,
    pub sample_count: HashMap<(String, Target), u32>,
    pub min_samples: u32,
}

impl Default for SchemaInferrer {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemaInferrer {
    pub fn new() -> Self {
        Self {
            schemas: HashMap::new(),
            sample_count: HashMap::new(),
            min_samples: 50,
        }
    }

    pub fn observe(&mut self, endpoint: &str, target: Target, body: &[u8]) {
        let Ok(value) = serde_json::from_slice::<Value>(body) else {
            return;
        };
        let Value::Object(obj) = value else { return };

        let key = (endpoint.to_string(), target);
        let total = self.sample_count.entry(key.clone()).or_insert(0);
        *total += 1;
        let current_total = *total;

        let schema = self.schemas.entry(key).or_default();

        for (field, val) in &obj {
            let stats = schema.entry(field.clone()).or_insert(FieldStats {
                field_type: infer_type(val),
                seen_count: 0,
                total_count: current_total,
                nullable: false,
            });
            stats.seen_count += 1;
            stats.total_count = current_total;
            if val.is_null() {
                stats.nullable = true;
            }
        }

        for stats in schema.values_mut() {
            stats.total_count = current_total;
        }
    }

    pub fn diff(&self, endpoint: &str) -> Option<StructuralSchemaDivergence> {
        let schema_a = self.schemas.get(&(endpoint.to_string(), Target::A))?;
        let schema_b = self.schemas.get(&(endpoint.to_string(), Target::B))?;

        let count_a = self
            .sample_count
            .get(&(endpoint.to_string(), Target::A))
            .copied()?;
        let count_b = self
            .sample_count
            .get(&(endpoint.to_string(), Target::B))
            .copied()?;

        if count_a < self.min_samples || count_b < self.min_samples {
            return None;
        }

        let mut fields_only_a = vec![];
        let mut fields_only_b = vec![];
        let mut type_mismatches = vec![];

        for (field, stats_a) in schema_a {
            match schema_b.get(field) {
                None => {
                    if stats_a.is_required() {
                        fields_only_a.push(field.clone());
                    }
                }
                Some(stats_b) => {
                    if stats_a.field_type != stats_b.field_type {
                        type_mismatches.push((
                            field.clone(),
                            stats_a.field_type.clone(),
                            stats_b.field_type.clone(),
                        ));
                    }
                }
            }
        }

        for (field, stats_b) in schema_b {
            if !schema_a.contains_key(field) && stats_b.is_required() {
                fields_only_b.push(field.clone());
            }
        }

        if fields_only_a.is_empty() && fields_only_b.is_empty() && type_mismatches.is_empty() {
            return None;
        }

        Some(StructuralSchemaDivergence {
            endpoint: endpoint.to_string(),
            fields_only_a,
            fields_only_b,
            type_mismatches,
        })
    }
}

fn infer_type(v: &Value) -> FieldType {
    match v {
        Value::String(_) => FieldType::String,
        Value::Number(n) => {
            if n.is_f64() {
                FieldType::Float
            } else {
                FieldType::Integer
            }
        }
        Value::Bool(_) => FieldType::Boolean,
        Value::Null => FieldType::Null,
        Value::Object(m) => FieldType::Object(
            m.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        FieldStats {
                            field_type: infer_type(v),
                            seen_count: 1,
                            total_count: 1,
                            nullable: v.is_null(),
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(a) => FieldType::Array(Box::new(
            a.first().map(infer_type).unwrap_or(FieldType::Null),
        )),
    }
}
