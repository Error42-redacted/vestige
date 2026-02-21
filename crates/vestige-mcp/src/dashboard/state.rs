//! Dashboard shared state

use std::sync::Arc;
use vestige_core::Storage;

/// Shared application state for the dashboard
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
}
