use std::sync::Arc;
use tokio::sync::RwLock;
use crate::artikel::Artikel;

pub mod routes;

#[derive(Clone)]
pub struct AppState {
    pub artikel: Arc<RwLock<Vec<Artikel>>>,
    pub password: String,
}
