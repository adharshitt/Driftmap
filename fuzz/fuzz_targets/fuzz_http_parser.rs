#![no_main]

use libfuzzer_sys::fuzz_target;
use driftmap_core::http::parse_http_message;

fuzz_target!(|data: &[u8]| {
    // Fuzz the HTTP parser to ensure it never panics on malformed network data
    let _ = parse_http_message(data);
});
