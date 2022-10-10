pub mod arp;
pub mod ip;

use self::{arp::ArpTable, ip::IPRoute};
use crate::{devices::NetDevice, util::List};
use std::{collections::VecDeque, sync::Arc};

#[derive(PartialEq, Debug)]
pub enum ProtocolType {
    Arp = 0x0806,
    IP = 0x0800,
    // IPV6 = 0x86dd,
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
    irq: i32,
    data: Option<Arc<Vec<u8>>>, // accessed from input/output threads for loopback
    len: usize,
}

impl ProtocolData {
    pub fn new(irq: i32, data: Option<Arc<Vec<u8>>>, len: usize) -> ProtocolData {
        ProtocolData { irq, data, len }
    }
}

pub struct NetProtocol {
    pub protocol_type: ProtocolType,
    pub input_head: VecDeque<ProtocolData>,
}

impl NetProtocol {
    pub fn new(t: ProtocolType) -> NetProtocol {
        NetProtocol {
            protocol_type: t,
            input_head: VecDeque::new(),
        }
    }

    /// Calls input handler for all data till a queue is empty.
    pub fn handle_input(
        &mut self,
        devices: &mut List<NetDevice>,
        arp_table: &mut ArpTable,
        ip_routes: &List<IPRoute>,
    ) {
        loop {
            if self.input_head.is_empty() {
                break;
            }
            let proto_data = self.input_head.pop_front().unwrap();
            let data = proto_data.data.unwrap();
            let len = proto_data.len;

            for device in devices.iter_mut() {
                if device.irq_entry.irq == proto_data.irq {
                    self.input(data.as_slice(), len, device, arp_table, ip_routes);
                    break;
                }
            }
        }
    }

    /// Handles input data per a protocol type.
    pub fn input(
        &self,
        data: &[u8],
        len: usize,
        device: &mut NetDevice,
        arp_table: &mut ArpTable,
        ip_routes: &List<IPRoute>,
    ) {
        // let parsed = u32::from_be_bytes(data.as_slice());
        match self.protocol_type {
            ProtocolType::Arp => {
                println!("Protocol: ARP | Received: {:02x?}", data);
                arp::input(data, len, device, arp_table, ip_routes).unwrap();
            }
            ProtocolType::IP => {
                println!("Protocol: IP | Received: {:x?}", data);
                ip::input(data, len, device, arp_table, ip_routes).unwrap();
            }
            ProtocolType::Unknown => {
                println!("Protocol: Unknown | Received: {:x?}", data);
            }
        }
        println!("======Handled an input=======")
    }
}
