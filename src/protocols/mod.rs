pub mod arp;
pub mod ip;

use self::{
    arp::ArpTable,
    ip::{tcp::TcpPcbs, udp::UdpPcbs, IPHeaderIdManager, IPRoutes},
};
use crate::{
    devices::{NetDevice, NetDevices},
    utils::list::List,
};
use log::{info, trace};
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
        // proto_stack: &mut ProtocolStack,
        devices: &mut NetDevices,
        contexts: &mut ProtocolContexts,
        pcbs: &mut ControlBlocks,
    ) {
        loop {
            if self.input_head.is_empty() {
                break;
            }
            let proto_data = self.input_head.pop_front().unwrap();
            let data = proto_data.data.unwrap();
            let len = proto_data.len;

            // let devices = proto_stack.devices.lock().unwrap();
            for device in devices.entries.iter_mut() {
                if device.irq_entry.irq == proto_data.irq {
                    self.input(data.as_slice(), len, device, contexts, pcbs);
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
        contexts: &mut ProtocolContexts,
        pcbs: &mut ControlBlocks,
    ) {
        // let parsed = u32::from_be_bytes(data.as_slice());
        info!("Protocol: ----Start of Input----");
        match self.protocol_type {
            ProtocolType::Arp => {
                trace!("Protocol: ARP | Received: {:02x?}", data);
                arp::input(data, len, device, contexts).unwrap();
            }
            ProtocolType::IP => {
                trace!("Protocol: IP | Received: {:02x?}", data);
                ip::input(data, len, device, contexts, pcbs).unwrap();
            }
            ProtocolType::Unknown => {
                trace!("Protocol: Unknown | Received: {:x?}", data);
            }
        }
        info!("Protocol: ----End of Input----\n")
    }
}

pub struct NetProtocols {
    pub entries: List<NetProtocol>,
}

impl NetProtocols {
    pub fn new() -> NetProtocols {
        NetProtocols {
            entries: List::<NetProtocol>::new(),
        }
    }

    pub fn register(&mut self, protocol: NetProtocol) {
        self.entries.push(protocol);
    }

    pub fn handle_data(
        &mut self,
        devices: &mut NetDevices,
        contexts: &mut ProtocolContexts,
        pcbs: &mut ControlBlocks,
    ) {
        for protocol in self.entries.iter_mut() {
            protocol.handle_input(devices, contexts, pcbs);
        }
    }
}
pub struct ProtocolContexts {
    pub arp_table: ArpTable,
    pub ip_routes: IPRoutes,
    pub ip_id_manager: IPHeaderIdManager,
}

pub struct ControlBlocks {
    pub udp_pcbs: UdpPcbs,
    pub tcp_pcbs: TcpPcbs,
}

impl ControlBlocks {
    pub fn new() -> ControlBlocks {
        ControlBlocks {
            udp_pcbs: UdpPcbs::new(),
            tcp_pcbs: TcpPcbs::new(),
        }
    }
}
