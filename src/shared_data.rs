use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct SharedData {
    pub counter: u64,          // 递增的序列号
    pub messages: Vec<String>, // 存储的消息
}

impl SharedData {
    pub fn new() -> Self {
        SharedData {
            counter: 1, // 序号从 1 开始
            messages: Vec::new(),
        }
    }

    pub fn add_message(&mut self, msg: String) -> u64 {
        let seq_num = self.counter;
        self.counter += 1;
        self.messages.push(msg);
        seq_num
    }

    pub fn get_messages_from(&self, start_seq: u64) -> Vec<String> {
        let mut result = Vec::new();
        for (i, msg) in self.messages.iter().enumerate() {
            if (i as u64 + 1) >= start_seq {
                result.push(msg.clone());
            }
        }
        result
    }
}
