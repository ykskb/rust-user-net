pub mod arp;
pub mod ip;

use std::{collections::VecDeque, sync::Arc};

use crate::{devices::NetDevice, util::List};

use self::arp::ArpTable;

#[derive(PartialEq)]
pub enum ProtocolType {
    Arp = 0x0806,
    // ICMP,
    IP = 0x0800,
    // IPV6 = 0x86dd,
    // TCP,
    // UDP,
    Unknown,
}

impl ProtocolType {
    pub fn from_u16(value: u16) -> ProtocolType {
        match value {
            0x0800 => ProtocolType::IP,
            0x0806 => ProtocolType::Arp,
            _ => ProtocolType::Unknown,
        }
    }
}

pub struct ProtocolData {
    data: Option<Arc<Vec<u8>>>, // accessed from input/output threads for loopback
    irq: i32,
}

impl ProtocolData {
    pub fn new(data: Option<Arc<Vec<u8>>>, irq: i32) -> ProtocolData {
        ProtocolData { data, irq }
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
    pub fn handle_input(&mut self, devices: &mut List<NetDevice>, arp_table: &mut ArpTable) {
        loop {
            if self.input_head.is_empty() {
                break;
            }
            let data = self.input_head.pop_front().unwrap();
            for device in devices.iter_mut() {
                if device.irq_entry.irq == data.irq {
                    self.input(data, device, arp_table);
                    break;
                }
            }
        }
    }

    /// Handles input data per a protocol type.
    pub fn input(&self, data: ProtocolData, device: &mut NetDevice, arp_table: &mut ArpTable) {
        let data_rc = data.data.unwrap();
        let data = data_rc.as_ref();
        // let parsed = u32::from_be_bytes(data.as_slice());
        match self.protocol_type {
            ProtocolType::IP => {
                println!("Protocol: IP | Received: {:?}", data);
            }
            ProtocolType::Arp => {
                println!("Protocol: ARP | Received: {:?}", data);
                arp::input(data, device, arp_table);
            }
            ProtocolType::Unknown => {
                println!("Protocol: Unknown | Received: {:?}", data);
            }
        }
    }
}
