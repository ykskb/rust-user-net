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

#[repr(packed)]
pub struct IPHeader {
    ver_len: u8,      // version (4 bits) + IHL (4 bits)
    service_type: u8, // | Precedence: 3 | Delay: 1 | Throughput: 1 | Reliability: 1 | Reserved: 2 |
    total_len: u16,
    id: u16,
    offset: u16, // flags: | 0 | Don't fragment: 1 | More fragment: 1 | + fragment offset (13 bits)
    ttl: u8,
    protocol: u8,
    check_sum: u16,
    src: IPAdress,
    dst: IPAdress,
    opts: [u8],
}
