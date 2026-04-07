use driftmap_core::scorer::{BehavioralDivergenceScore, DashboardUpdate, SystemHealth};
use tokio::sync::watch;

#[derive(Clone, PartialEq)]
pub enum SortMode {
    ByScore,
    ByName,
    ByRequests,
}

use ratatui::widgets::TableState;

pub struct App {
    pub scores: Vec<BehavioralDivergenceScore>,
    pub health: SystemHealth,
    pub selected: usize,
    pub table_state: TableState,
    pub sort_by: SortMode,
    pub filter: Option<String>,
    pub score_rx: watch::Receiver<DashboardUpdate>,
    pub target_a: String,
    pub target_b: String,
    pub input: String,
}

impl App {
    pub fn new(score_rx: watch::Receiver<DashboardUpdate>) -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0));
        Self {
            scores: Vec::new(),
            health: SystemHealth::default(),
            selected: 0,
            table_state,
            sort_by: SortMode::ByScore,
            filter: None,
            score_rx,
            target_a: "Stable".to_string(),
            target_b: "Candidate".to_string(),
            input: String::new(),
        }
    }
}
