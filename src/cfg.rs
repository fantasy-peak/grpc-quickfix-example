use config::{Config, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GwConfig {
    pub fix_cfg: String,
    pub session_id: String,
}
