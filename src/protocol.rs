use std::collections::VecDeque;

enum ProtocolType {
    IP,
}

pub struct NetProtocol {
    protocol_type: ProtocolType,
    input_head: VecDeque<u32>,
    handle: fn(),
}

impl NetProtocol {
    fn new(t: ProtocolType, handle: fn()) -> NetProtocol {
        NetProtocol {
            protocol_type: t,
            input_head: VecDeque::new(),
            handle,
        }
    }
    fn input(&self, data: u8, len: usize) {
        match self.protocol_type {
            ProtocolType::IP => {
                println!("data: {} length: {}", data, len);
            }
        }
    }
}
