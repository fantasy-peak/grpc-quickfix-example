use std::collections::HashSet;

pub struct OrderManager {
    orders: HashSet<String>,
}

impl OrderManager {
    pub fn new() -> Self {
        OrderManager {
            orders: HashSet::new(),
        }
    }

    pub async fn check_and_insert(&mut self, order_id: &str) -> bool {
        if self.orders.contains(order_id) {
            true
        } else {
            self.orders.insert(order_id.to_string()); // 插入 String（必须将 &str 转为 String）
            false
        }
    }
}
