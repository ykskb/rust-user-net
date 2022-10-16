use super::{
    IPAdress, IPEndpoint, IPInterface, IPProtocolType, IPRoute, IP_ADDR_ANY, IP_PAYLOAD_MAX_SIZE,
};
use crate::{
    devices::NetDevice,
    protocols::arp::ArpTable,
    util::{bytes_to_struct, cksum16, le_to_be_u16, to_u8_slice, List},
};
use std::{collections::VecDeque, mem::size_of};

const UDP_PCB_COUNT: usize = 16;

struct PseudoHeader {
    src: IPAdress,
    dst: IPAdress,
    zero: u8,
    protocol: u8,
    len: u16,
}

struct UdpHeader {
    src_port: u16,
    dst_port: u16,
    len: u16,
    checksum: u16,
}

// PCB: protocol control block

#[derive(PartialEq)]
enum UdpPcbState {
    Free,
    Open,
    Closing,
}

// Protocol control block
struct UdpPcb {
    state: UdpPcbState,
    local_endpoint: IPEndpoint,
    data_entries: VecDeque<UdpDataEntry>,
}

struct UdpDataEntry {
    remote_endpoint: IPEndpoint,
    len: usize,
    data: Vec<u8>,
}

struct UdpPcbs {
    entries: List<UdpPcb>,
}

impl UdpPcbs {
    pub fn new() -> UdpPcbs {
        UdpPcbs {
            entries: List::<UdpPcb>::new(),
        }
    }

    pub fn add_pcb(&mut self, local_endpoint: IPEndpoint) {
        let new_pcb = UdpPcb {
            state: UdpPcbState::Open,
            local_endpoint,
            data_entries: VecDeque::new(),
        };
        self.entries.push(new_pcb);
    }

    pub fn delete_pcb(&mut self, local_endpoint: IPEndpoint) {}

    pub fn select_open_pcb(&mut self, host_addr: IPAdress, host_port: u16) -> Option<&mut UdpPcb> {
        for pcb in self.entries.iter_mut() {
            if pcb.state == UdpPcbState::Open {
                if (pcb.local_endpoint.address == IP_ADDR_ANY
                    || host_addr == IP_ADDR_ANY
                    || pcb.local_endpoint.address == host_addr)
                    && pcb.local_endpoint.port == host_port
                {
                    return Some(pcb);
                }
            }
        }
        None
    }
}

pub fn input(
    data: &[u8],
    len: usize,
    src: IPAdress,
    dst: IPAdress,
    device: &mut NetDevice,
    iface: &IPInterface,
) -> Result<(), ()> {
    let header = unsafe { bytes_to_struct::<UdpHeader>(data) };
    let pseudo_header = PseudoHeader {
        src,
        dst,
        zero: 0,
        protocol: IPProtocolType::Udp as u8,
        len: le_to_be_u16(len as u16),
    };
    let pseudo_hdr_bytes = unsafe { to_u8_slice(&pseudo_header) };
    let pseudo_sum = cksum16(pseudo_hdr_bytes, pseudo_hdr_bytes.len(), 0);
    let sum = cksum16(data, len, !pseudo_sum as u32);
    if sum != 0 {
        println!("UDP input checksum failure: value = {sum}");
        return Err(());
    }

    println!(
        "UDP input: source port = {:?} destination port: {:?}",
        header.src_port, header.dst_port
    );

    Ok(())
}

pub fn output(
    src: IPEndpoint,
    dst: IPEndpoint,
    mut udp_data: Vec<u8>,
    len: usize,
    device: &mut NetDevice,
    arp_table: &mut ArpTable,
    ip_routes: &List<IPRoute>,
) {
    println!("UDP outpu");
    let udp_hdr_size = size_of::<UdpHeader>();
    if len > (IP_PAYLOAD_MAX_SIZE - udp_hdr_size) {
        panic!("UDP output error: data too big");
    }
    let total_len = udp_hdr_size + len;
    let total_len_in_be = le_to_be_u16(total_len as u16);
    let udp_header = UdpHeader {
        src_port: src.port,
        dst_port: dst.port,
        len: total_len_in_be,
        checksum: 0,
    };
    let pseudo_hdr = PseudoHeader {
        src: src.address,
        dst: dst.address,
        zero: 0,
        protocol: IPProtocolType::Udp as u8,
        len: total_len_in_be,
    };
    let pseudo_hdr_bytes = unsafe { to_u8_slice(&pseudo_hdr) };
    let pseudo_sum = cksum16(pseudo_hdr_bytes, pseudo_hdr_bytes.len(), 0);

    let udp_hdr_bytes = unsafe { to_u8_slice::<UdpHeader>(&udp_header) };
    let mut data = udp_hdr_bytes.to_vec();
    data.append(&mut udp_data);
    // Update checksum
    let sum = cksum16(&data, total_len, !pseudo_sum as u32);
    data[6] = ((sum & 0xff00) >> 8) as u8;
    data[7] = (sum & 0xff) as u8;

    super::output(
        IPProtocolType::Udp,
        data,
        src.address,
        dst.address,
        device,
        arp_table,
        ip_routes,
    )
    .unwrap();
}
