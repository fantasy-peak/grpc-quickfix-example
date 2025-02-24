use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct GwConfig {
    pub address: String,
    pub fix_cfg: String,
    pub begin_string: String,
    pub sender_comp_id: String,
    pub target_comp_id: String,
    pub interval: u64,
}
