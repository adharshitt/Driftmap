#![no_std]

#[repr(C)]
pub struct Request {
    pub method:     *const u8,
    pub method_len: usize,
    pub path:       *const u8,
    pub path_len:   usize,
    pub body:       *const u8,
    pub body_len:   usize,
}

#[repr(C)]
pub struct Response {
    pub status:   u16,
    pub body:     *const u8,
    pub body_len: usize,
}

#[repr(C)]
pub struct PluginScore {
    pub score:          f32,
    pub annotation:     *const u8,
    pub annotation_len: usize,
}

pub trait DriftPlugin {
    fn score_pair(req_a: &Request, res_a: &Response,
                  req_b: &Request, res_b: &Response) -> PluginScore;
}

#[macro_export]
macro_rules! export_plugin {
    ($ty:ty) => {
        #[no_mangle]
        pub extern "C" fn score_pair(
            req_a_method: *const u8, req_a_method_len: usize,
            req_a_path: *const u8, req_a_path_len: usize,
            req_a_body: *const u8, req_a_body_len: usize,
            res_a_status: u16, res_a_body: *const u8, res_a_body_len: usize,
            req_b_method: *const u8, req_b_method_len: usize,
            req_b_path: *const u8, req_b_path_len: usize,
            req_b_body: *const u8, req_b_body_len: usize,
            res_b_status: u16, res_b_body: *const u8, res_b_body_len: usize,
        ) -> f32 {
            let req_a = $crate::Request {
                method: req_a_method, method_len: req_a_method_len,
                path: req_a_path, path_len: req_a_path_len,
                body: req_a_body, body_len: req_a_body_len,
            };
            let res_a = $crate::Response {
                status: res_a_status,
                body: res_a_body, body_len: res_a_body_len,
            };
            let req_b = $crate::Request {
                method: req_b_method, method_len: req_b_method_len,
                path: req_b_path, path_len: req_b_path_len,
                body: req_b_body, body_len: req_b_body_len,
            };
            let res_b = $crate::Response {
                status: res_b_status,
                body: res_b_body, body_len: res_b_body_len,
            };

            let result = <$ty as $crate::DriftPlugin>::score_pair(&req_a, &res_a, &req_b, &res_b);
            result.score
        }

        // Standard memory allocator for WASM so the host can pass data in
        #[no_mangle]
        pub extern "C" fn alloc(size: usize) -> *mut u8 {
            let mut buf = std::vec::Vec::with_capacity(size);
            let ptr = buf.as_mut_ptr();
            std::mem::forget(buf);
            ptr
        }
    }
}
