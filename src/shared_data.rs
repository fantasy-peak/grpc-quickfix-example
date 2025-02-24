#[derive(Debug)]
pub struct SharedData {
    pub counter: u64,
    pub messages: Vec<String>,
}

impl SharedData {
    pub fn new() -> Self {
        SharedData {
            counter: 1,
            messages: Vec::new(),
        }
    }

    pub fn add_message(&mut self, msg: String) -> u64 {
        let seq_num = self.counter;
        self.counter += 1;
        self.messages.push(msg);
        seq_num
    }

    pub fn get_messages_from(&self, start_seq: u64) -> Vec<SharedData> {
        let mut result = Vec::new();
        for (i, msg) in self.messages.iter().enumerate() {
            if (i as u64 + 1) >= start_seq {
                let mut sd = SharedData::new();
                sd.counter = i as u64 + 1;
                sd.messages.push(msg.clone());
                result.push(sd);
            }
        }
        result
    }
}
