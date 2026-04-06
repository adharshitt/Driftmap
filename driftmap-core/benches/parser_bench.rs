use criterion::{black_box, criterion_group, criterion_main, Criterion};
use driftmap_core::http::parse_http_message;

fn bench_http_parser(c: &mut Criterion) {
    let payload = b"GET /api/users/1234-5678-9012-3456 HTTP/1.1\r\nHost: example.com\r\nUser-Agent: curl/7.68.0\r\nAccept: */*\r\n\r\n";
    
    c.bench_function("parse_http_message GET", |b| {
        b.iter(|| parse_http_message(black_box(payload)))
    });

    let payload_post = b"POST /api/orders HTTP/1.1\r\nHost: example.com\r\nContent-Length: 27\r\nContent-Type: application/json\r\n\r\n{\"id\": 123, \"status\": \"ok\"}";
    c.bench_function("parse_http_message POST", |b| {
        b.iter(|| parse_http_message(black_box(payload_post)))
    });
}

criterion_group!(benches, bench_http_parser);
criterion_main!(benches);
