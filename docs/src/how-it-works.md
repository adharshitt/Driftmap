# How DriftMap Works

DriftMap is built on the concept of **Semantic Equivalence**. Two systems can return different JSON key orderings, distinct timestamps, or varying latency profiles and still be functionally identical. Conversely, they can return identical `200 OK` status codes and be entirely broken.

## The Core Pipeline

1. **eBPF Capture:** A Traffic Control (TC) hook intercepts raw TCP packets post-routing with near-zero overhead.
2. **Reassembly:** Packets are buffered and reassembled into complete HTTP messages using a streaming parser.
3. **Matching:** Requests are paired across environments using templatized paths (e.g., `/users/:id`) within a 500ms sliding window.
4. **Semantic Scoring:** A `t-Digest` tracking system computes latency and distribution deltas, while a recursive inference engine compares JSON schema structures.
