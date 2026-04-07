use criterion::{black_box, criterion_group, criterion_main, Criterion};
use driftmap_core::matcher::Target;
use driftmap_core::schema::SchemaInferrer;

fn bench_schema_inferrer(c: &mut Criterion) {
    let mut inferrer = SchemaInferrer::new();
    let payload = b"{\"id\": 12345, \"user\": {\"name\": \"Alice\", \"email\": \"alice@example.com\", \"active\": true}, \"tags\": [\"admin\", \"user\"], \"metadata\": null}";

    c.bench_function("schema_inferrer_observe", |b| {
        b.iter(|| {
            inferrer.observe(
                black_box("GET /api/users/:id"),
                black_box(Target::A),
                black_box(payload),
            );
        })
    });
}

criterion_group!(benches, bench_schema_inferrer);
criterion_main!(benches);
