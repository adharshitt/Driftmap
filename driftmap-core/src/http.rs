use httparse;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method:  String,
    pub path:    String,
    pub path_template: String,
    pub headers: Vec<(String, String)>,
    pub body:    Vec<u8>,
    pub captured_at: Instant,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status:  u16,
    pub headers: Vec<(String, String)>,
    pub body:    Vec<u8>,
    pub captured_at: Instant,
}

#[derive(Debug, Clone)]
pub enum HttpMessage {
    Request(HttpRequest),
    Response(HttpResponse),
}

pub fn parse_http_message(raw: &[u8]) -> Option<HttpMessage> {
    if raw.starts_with(b"HTTP/") {
        parse_response(raw).map(HttpMessage::Response)
    } else {
        parse_request(raw).map(HttpMessage::Request)
    }
}

fn parse_request(raw: &[u8]) -> Option<HttpRequest> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);

    match req.parse(raw) {
        Ok(httparse::Status::Complete(header_len)) => {
            let method = req.method?.to_string();
            let path   = req.path?.to_string();
            let path_template = templatize_path(&path);

            let hdrs: Vec<(String,String)> = headers.iter()
                .take_while(|h| !h.name.is_empty())
                .map(|h| (
                    h.name.to_lowercase(),
                    String::from_utf8_lossy(h.value).to_string()
                ))
                .collect();

            let content_length: usize = hdrs.iter()
                .find(|(k,_)| k == "content-length")
                .and_then(|(_,v)| v.parse().ok())
                .unwrap_or(0);

            let body_end = (header_len + content_length).min(raw.len());
            let body = raw[header_len..body_end].to_vec();

            Some(HttpRequest { method, path, path_template, headers: hdrs, body })
        }
        _ => None,
    }
}

fn parse_response(raw: &[u8]) -> Option<HttpResponse> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut res = httparse::Response::new(&mut headers);

    match res.parse(raw) {
        Ok(httparse::Status::Complete(header_len)) => {
            let status = res.code?;
            
            let hdrs: Vec<(String,String)> = headers.iter()
                .take_while(|h| !h.name.is_empty())
                .map(|h| (
                    h.name.to_lowercase(),
                    String::from_utf8_lossy(h.value).to_string()
                ))
                .collect();

            let content_length: usize = hdrs.iter()
                .find(|(k,_)| k == "content-length")
                .and_then(|(_,v)| v.parse().ok())
                .unwrap_or(0);

            let body_end = (header_len + content_length).min(raw.len());
            let body = raw[header_len..body_end].to_vec();

            Some(HttpResponse { status, headers: hdrs, body })
        }
        _ => None,
    }
}

pub fn templatize_path(path: &str) -> String {
    let path_only = path.split('?').next().unwrap_or(path);
    path_only.split('/')
        .map(|segment| {
            if segment.is_empty() { return ""; }
            if segment.chars().all(|c| c.is_ascii_digit())
               || segment.len() == 36 && segment.contains('-')
               || (segment.len() > 8 && segment.chars().all(|c| c.is_ascii_hexdigit()))
            {
                ":id"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}
