use driftmap_core::scorer::BehavioralDivergenceScore;
use tokio::sync::watch;

#[derive(Clone, PartialEq)]
pub enum SortMode {
    ByScore,
    ByName,
    ByRequests,
}

pub struct App {
    pub scores: Vec<BehavioralDivergenceScore>,
    pub selected: usize,
    pub sort_by: SortMode,
    pub filter: Option<String>,
    pub score_rx: watch::Receiver<Vec<BehavioralDivergenceScore>>,
}

impl App {
    pub fn new(score_rx: watch::Receiver<Vec<BehavioralDivergenceScore>>) -> Self {
        Self {
            scores: Vec::new(),
            selected: 0,
            sort_by: SortMode::ByScore,
            filter: None,
            score_rx,
        }
    }
}
