use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use crate::http::{HttpRequest, HttpResponse};

#[derive(Debug, Clone)]
pub enum Target {
    A,
    B,
}

#[derive(Debug, Clone)]
pub struct PendingRequest {
    pub request: HttpRequest,
    pub response: HttpResponse,
    pub arrived_at: Instant,
    pub captured_at: Instant,
}

#[derive(Debug, Clone)]
pub struct MatchedPair {
    pub endpoint: String,
    pub req_a: HttpRequest,
    pub res_a: HttpResponse,
    pub req_b: HttpRequest,
    pub res_b: HttpResponse,
}

pub struct Matcher {
    /// Requests from A waiting for a matching request from B
    pending_a: HashMap<String, VecDeque<PendingRequest>>,
    /// Requests from B waiting for a matching request from A
    pending_b: HashMap<String, VecDeque<PendingRequest>>,

    window: Duration,
    tx: mpsc::Sender<MatchedPair>,
}

impl Matcher {
    pub fn new(tx: mpsc::Sender<MatchedPair>) -> Self {
        Self {
            pending_a: HashMap::new(),
            pending_b: HashMap::new(),
            window: Duration::from_millis(500),
            tx,
        }
    }

    pub fn ingest(&mut self, target: Target, req: HttpRequest, res: HttpResponse) {
        let key = format!("{} {}", req.method, req.path_template);

        let (my_pending, their_pending) = match target {
            Target::A => (&mut self.pending_a, &mut self.pending_b),
            Target::B => (&mut self.pending_b, &mut self.pending_a),
        };

        // Check if there is a match in the other queue
        if let Some(queue) = their_pending.get_mut(&key) {
            while let Some(front) = queue.front() {
                if front.arrived_at.elapsed() > self.window {
                    queue.pop_front();
                    continue;
                }

                // Match found!
                let matched = queue.pop_front().unwrap();
                let pair = match target {
                    Target::A => MatchedPair {
                        endpoint: key.clone(),
                        req_a: req,
                        res_a: res,
                        req_b: matched.request,
                        res_b: matched.response,
                    },
                    Target::B => MatchedPair {
                        endpoint: key.clone(),
                        req_a: matched.request,
                        res_a: matched.response,
                        req_b: req,
                        res_b: res,
                    },
                };

                let _ = self.tx.try_send(pair);
                return;
            }
        }

        // No match found, add to pending
        let queue = my_pending
            .entry(key)
            .or_insert_with(VecDeque::new);
            
        // HashDoS Prevention: Cap pending queue at 100 requests per endpoint
        if queue.len() >= 100 {
            queue.pop_front();
        }
        queue.push_back(PendingRequest {
                request: req,
                response: res,
                arrived_at: Instant::now(),
            });
    }

    pub fn gc(&mut self) {
        let cutoff = self.window;
        for queue in self.pending_a.values_mut().chain(self.pending_b.values_mut()) {
            while queue.front().map(|p| p.arrived_at.elapsed() > cutoff).unwrap_or(false) {
                queue.pop_front();
            }
        }
    }
}
