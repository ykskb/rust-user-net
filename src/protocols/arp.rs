use std::{
    collections::{HashMap, HashSet},
    convert::TryInto,
    time::SystemTime,
};

use crate::{
    devices::{ethernet::ETH_ADDR_LEN, NetDevice, NetDeviceType},
    net::{IPAdress, IPInterface, NetInterfaceFamily},
    protocols::arp,
    util::{be_to_le_u16, bytes_to_struct, le_to_be_u16, to_u8_slice},
};

use super::ip::IP_ADDR_LEN;
use super::ProtocolType;

const ARP_HW_SPACE_ETHER: u16 = 0x0001;
const ARP_PROTO_SPACE_IP: u16 = 0x0800;
const ARP_OP_REQUEST: u16 = 0x0001;
const ARP_OP_REPLY: u16 = 0x0002;

const ARP_CACHE_TIMEOUT_SECS: u64 = 60 * 60 * 4; // timeout: 4hr

#[derive(PartialEq, Eq, Hash)]
enum ArpTableEntryState {
    Incomplete,
    Resolved,
    Static,
}

#[derive(PartialEq, Eq, Hash)]
pub struct ArpTableEntry {
    state: ArpTableEntryState,
    proto_address: IPAdress,
    hw_address: [u8; ETH_ADDR_LEN],
    timestamp: SystemTime,
}

pub struct ArpTable {
    entries: HashMap<IPAdress, ArpTableEntry>,
}

impl ArpTable {
    pub fn new() -> ArpTable {
        ArpTable {
            entries: HashMap::<IPAdress, ArpTableEntry>::new(),
        }
    }

    pub fn get(&mut self, ip: IPAdress) -> Option<[u8; 6]> {
        let map_entry = self.entries.get(&ip);
        if let Some(entry) = map_entry {
            let dur = entry.timestamp.elapsed().unwrap();
            if dur.as_secs() > ARP_CACHE_TIMEOUT_SECS {
                self.entries.remove(&ip);
                return None;
            } else {
                return Some(entry.hw_address);
            }
        }
        None
    }

    pub fn update(&mut self, ip: IPAdress, resolved: [u8; ETH_ADDR_LEN]) {
        let map_entry = self.entries.get(&ip);
        if let Some(_entry) = map_entry {
            self.entries.remove(&ip);
        }
        self.entries.insert(
            ip,
            ArpTableEntry {
                state: ArpTableEntryState::Resolved,
                proto_address: ip,
                hw_address: resolved,
                timestamp: SystemTime::now(),
            },
        );
    }
}

#[repr(packed)]
struct ArpHeader {
    hw_addr_space: u16,    // Hardware address space: 0x0001 for Ethernet
    proto_addr_space: u16, // Protocol address space: 0x0800 for IP
    hw_addr_len: u8,       // Hardware address length: Ethernet address size
    proto_addr_len: u8,    // Protocol address length: IP address size
    op: u16,               // Operation code: REQUEST or REPLY
}

#[repr(packed)]
struct ArpMessage {
    header: ArpHeader,
    sender_hw_addr: [u8; ETH_ADDR_LEN],
    sender_proto_addr: [u8; IP_ADDR_LEN],
    target_hw_addr: [u8; ETH_ADDR_LEN],
    target_proto_addr: [u8; IP_ADDR_LEN],
}

pub fn arp_request(device: &mut NetDevice, target_ip: IPAdress) -> Result<(), ()> {
    let request_header = ArpHeader {
        hw_addr_space: le_to_be_u16(ARP_HW_SPACE_ETHER),
        hw_addr_len: ETH_ADDR_LEN as u8,
        proto_addr_space: le_to_be_u16(ARP_PROTO_SPACE_IP),
        proto_addr_len: IP_ADDR_LEN as u8,
        op: le_to_be_u16(ARP_OP_REQUEST),
    };
    let interface_unicast_ip = device
        .get_interface_unicast(NetInterfaceFamily::IP)
        .unwrap();
    let request_msg = ArpMessage {
        header: request_header,
        sender_hw_addr: device.address[..6]
            .try_into()
            .expect("ARP request failure: sender hw address."),
        sender_proto_addr: interface_unicast_ip.to_le_bytes(),
        target_hw_addr: [0; 6],
        target_proto_addr: target_ip.to_le_bytes(),
    };
    let data = unsafe { to_u8_slice::<ArpMessage>(&request_msg) };
    println!("Sending ARP request for IP: {target_ip}");
    device.transmit(
        ProtocolType::Arp,
        data.to_vec(),
        data.len(),
        device.broadcast[..6]
            .try_into()
            .expect("ARP reply failure: broadcast address."),
    )
}

pub fn arp_reply(
    device: &mut NetDevice,
    target_hw_addr: [u8; ETH_ADDR_LEN],
    target_ip: IPAdress,
    destination_hw_addr: [u8; ETH_ADDR_LEN],
) -> Result<(), ()> {
    let reply_header = ArpHeader {
        hw_addr_space: le_to_be_u16(ARP_HW_SPACE_ETHER),
        hw_addr_len: ETH_ADDR_LEN as u8,
        proto_addr_space: le_to_be_u16(ARP_PROTO_SPACE_IP),
        proto_addr_len: IP_ADDR_LEN as u8,
        op: le_to_be_u16(ARP_OP_REPLY),
    };

    let interface_unicast_ip = device
        .get_interface_unicast(NetInterfaceFamily::IP)
        .unwrap();
    let reply_msg = ArpMessage {
        header: reply_header,
        sender_hw_addr: device.address[..6]
            .try_into()
            .expect("ARP reply failure: sender hw address."),
        sender_proto_addr: interface_unicast_ip.to_le_bytes(),
        target_hw_addr,
        target_proto_addr: target_ip.to_le_bytes(),
    };

    let data = unsafe { to_u8_slice::<ArpMessage>(&reply_msg) };
    device.transmit(
        ProtocolType::Arp,
        data.to_vec(),
        data.len(),
        destination_hw_addr,
    )
}

pub fn input(data: &[u8], device: &mut NetDevice, arp_table: &mut ArpTable) -> Result<(), ()> {
    let msg = unsafe { bytes_to_struct::<ArpMessage>(data) };

    if msg.header.hw_addr_space != ARP_HW_SPACE_ETHER
        || msg.header.hw_addr_len as usize != ETH_ADDR_LEN
    {
        return Err(());
    }
    if msg.header.proto_addr_space != ARP_PROTO_SPACE_IP
        || msg.header.proto_addr_len as usize != IP_ADDR_LEN
    {
        return Err(());
    }

    let target_ip = unsafe { bytes_to_struct::<u32>(&msg.target_proto_addr) };
    let interface_unicast = device
        .get_interface_unicast(NetInterfaceFamily::IP)
        .unwrap();
    if interface_unicast != target_ip {
        println!("ARP Input: target IP: {target_ip} not matching with interface unicast IP.");
    } else {
        // Update or insert ARP Table with sender addresses
        let sender_ip = unsafe { bytes_to_struct::<u32>(&msg.sender_proto_addr) };
        arp_table.update(sender_ip, msg.sender_hw_addr);
        println!("ARP resolved for IP: {sender_ip}");

        // Reply in case of ARP Request
        if be_to_le_u16(msg.header.op) == ARP_OP_REQUEST {
            let sender_ip = unsafe { bytes_to_struct::<u32>(&msg.sender_proto_addr) };
            return arp_reply(device, msg.sender_hw_addr, sender_ip, msg.sender_hw_addr);
        }
    }

    Ok(())
}

pub fn arp_resolve(
    device: &mut NetDevice,
    arp_table: &mut ArpTable,
    target_ip: IPAdress,
) -> Result<Option<[u8; ETH_ADDR_LEN]>, ()> {
    if device.device_type != NetDeviceType::Ethernet {
        return Err(());
    }
    // TODO: Interface family check if IP
    if let Some(hw_addr) = arp_table.get(target_ip) {
        Ok(Some(hw_addr))
    } else if arp_request(device, target_ip).is_ok() {
        Ok(None)
    } else {
        Err(())
    }
}
