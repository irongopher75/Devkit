use std::sync::Arc;

use crate::config::DeckConfig;
use crate::events::EventBus;
use crate::supervisor::Supervisor;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<DeckConfig>,
    pub supervisor: Arc<Supervisor>,
    pub bus: Arc<EventBus>,
    pub token: Arc<String>,
    pub port: u16,
}
