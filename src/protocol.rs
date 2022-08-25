use std::{collections::VecDeque, sync::Arc};

use crate::net::IPAdress;

#[derive(PartialEq)]
pub enum ProtocolType {
    // ARP = 0x0806,
    // ICMP,
    IP = 0x0800,
    // IPV6 = 0x86dd,
    // TCP,
    // UDP,
}

pub struct ProtocolData {
    data: Option<Arc<[u8]>>, // accessed from input/output threads for loopback
}

impl ProtocolData {
    pub fn new(data: Option<Arc<[u8]>>) -> ProtocolData {
        ProtocolData { data }
    }
}

pub struct NetProtocol {
    pub protocol_type: ProtocolType,
    pub input_head: VecDeque<ProtocolData>,
    pub next_protocol: Option<Box<NetProtocol>>,
}

impl NetProtocol {
    pub fn new(t: ProtocolType) -> NetProtocol {
        NetProtocol {
            protocol_type: t,
            input_head: VecDeque::new(),
            next_protocol: None,
        }
    }

    /// Calls input handler for all data till a queue is empty.
    pub fn handle_input(&mut self) {
        loop {
            if self.input_head.is_empty() {
                break;
            }
            let data = self.input_head.pop_front().unwrap();
            self.input(data)
        }
    }

    /// Handles input data per a protocol type.
    pub fn input(&self, data: ProtocolData) {
        let data_rc = data.data.unwrap();
        let data = data_rc.as_ref();
        // let parsed = u32::from_be_bytes(data.as_ref());
        match self.protocol_type {
            ProtocolType::IP => {
                println!("Protocol: IP | Received: {:?}", data);
            }
        }
    }
}
