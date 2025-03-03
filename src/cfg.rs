use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub enum BrokerName {
    Broker1,
    Broker2,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GwConfig {
    pub address: String,
    pub fix_cfg: String,
    pub begin_string: String,
    pub sender_comp_id: String,
    pub target_comp_id: String,
    pub interval: u64,
    pub plugin_cfg_file: String,
    pub broker_name: BrokerName,
}
