use crate::server::fantasy::RequestMessage;
use fantasy_fix42::NewOrderSingle;

pub trait Plugin {
    fn convert_to_new_order_single(
        &self,
        order: &RequestMessage,
    ) -> Result<NewOrderSingle, quickfix::QuickFixError>;

    fn convert_to_order_cancel_replace_request(
        &self,
        order: &RequestMessage,
    ) -> Result<NewOrderSingle, quickfix::QuickFixError>;

    fn convert_to_order_cancel_request(
        &self,
        order: &RequestMessage,
    ) -> Result<NewOrderSingle, quickfix::QuickFixError>;
}
