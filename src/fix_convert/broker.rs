use fantasy_fix42::NewOrderSingle;
use fantasy_fix42::field_types::{HandlInst, OrdType, Side};
use quickfix::*;
use serde::de::value::Error;

use crate::fix_convert::gw_plugin::Plugin;
use crate::server::fantasy::RequestMessage;

struct BrokerCfg {
    pub path: String,
}

pub struct Broker {
    pub plugin_cfg_file: String,
    pub broker_cfg: BrokerCfg,
}

impl Broker {
    pub fn new(plugin_cfg_file: &str) -> Self {
        // Load custom file here
        Broker {
            plugin_cfg_file: plugin_cfg_file.to_string(),
            broker_cfg: BrokerCfg {
                path: plugin_cfg_file.to_string(),
            },
        }
    }
}

impl Plugin for Broker {
    fn convert_to_new_order_single(
        &self,
        req: &RequestMessage,
    ) -> Result<NewOrderSingle, quickfix::QuickFixError> {
        println!("[{}] convert_to_new_order_single", self.plugin_cfg_file);
        let now = chrono::Utc::now();
        let timestamp = now.format("%Y%m%d-%H:%M:%S%.3f").to_string();

        let mut order = NewOrderSingle::try_new(
            req.message.clone(),
            HandlInst::AutomatedExecutionNoIntervention,
            "USDJPY".to_string(),
            Side::Buy,
            timestamp,
            OrdType::Limit,
        )?;

        macro_rules! try_set {
            ($order:expr, $method:ident, $value:expr, $message:expr) => {
                if let Err(e) = $order.$method($value) {
                    eprintln!($message, e);
                    return Err(e);
                }
            };
        }
        try_set!(
            order,
            set_order_qty,
            14.0,
            "Failed to set order quantity: {}"
        );
        try_set!(
            order,
            set_symbol,
            "USDJPY".to_string(),
            "Failed to set symbol: {}"
        );
        try_set!(order, set_price, 893.123, "Failed to set price: {}");
        try_set!(
            order,
            set_account,
            "fantasy".to_string(),
            "Failed to set account: {}"
        );
        Ok(order)
    }

    fn convert_to_order_cancel_replace_request(
        &self,
        req: &RequestMessage,
    ) -> Result<NewOrderSingle, quickfix::QuickFixError> {
        return Err(QuickFixError::InvalidArgument("ssss".to_string()));
    }

    fn convert_to_order_cancel_request(
        &self,
        req: &RequestMessage,
    ) -> Result<NewOrderSingle, quickfix::QuickFixError> {
        return Err(QuickFixError::InvalidArgument("ssss".to_string()));
    }
}
