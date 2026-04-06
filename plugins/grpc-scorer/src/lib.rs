#![no_std]

use driftmap_plugin_sdk::{DriftPlugin, PluginScore, Request, Response};

pub struct GrpcScorer;

impl DriftPlugin for GrpcScorer {
    fn score_pair(
        _req_a: &Request,
        res_a: &Response,
        _req_b: &Request,
        res_b: &Response,
    ) -> PluginScore {
        let body_a = unsafe { core::slice::from_raw_parts(res_a.body, res_a.body_len) };
        let body_b = unsafe { core::slice::from_raw_parts(res_b.body, res_b.body_len) };

        // gRPC frames start with 1 byte (compression flag) and 4 bytes (length)
        if body_a.len() < 5 || body_b.len() < 5 {
            return PluginScore {
                score: if body_a == body_b { 0.0 } else { 1.0 },
                annotation: core::ptr::null(),
                annotation_len: 0,
            };
        }

        let len_a = u32::from_be_bytes([body_a[1], body_a[2], body_a[3], body_a[4]]) as usize;
        let len_b = u32::from_be_bytes([body_b[1], body_b[2], body_b[3], body_b[4]]) as usize;

        // If the declared protobuf lengths differ, the messages have drifted structurally.
        if len_a != len_b {
            return PluginScore {
                score: 1.0,
                annotation: core::ptr::null(),
                annotation_len: 0,
            };
        }

        // For MVP, if lengths match, do a strict byte comparison.
        // A full implementation would use a protobuf reflection library to decode here.
        let score = if body_a[5..5+len_a] == body_b[5..5+len_b] {
            0.0
        } else {
            // Bytes differ, but lengths match (e.g. a timestamp changed)
            0.5 
        };

        PluginScore {
            score,
            annotation: core::ptr::null(),
            annotation_len: 0,
        }
    }
}

driftmap_plugin_sdk::export_plugin!(GrpcScorer);
