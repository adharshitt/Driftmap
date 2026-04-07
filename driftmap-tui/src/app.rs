use driftmap_core::scorer::{BehavioralDivergenceScore, DashboardUpdate, SystemHealth};
use tokio::sync::watch;

#[derive(Clone, PartialEq)]
pub enum SortMode {
    ByScore,
    ByName,
    ByRequests,
}

pub struct App {
    pub scores: Vec<BehavioralDivergenceScore>,
    pub health: SystemHealth,
    pub selected: usize,
    pub sort_by: SortMode,
    pub filter: Option<String>,
    pub score_rx: watch::Receiver<DashboardUpdate>,
}

impl App {
    pub fn new(score_rx: watch::Receiver<DashboardUpdate>) -> Self {
        Self {
            scores: Vec::new(),
            health: SystemHealth::default(),
            selected: 0,
            sort_by: SortMode::ByScore,
            filter: None,
            score_rx,
        }
    }
}
