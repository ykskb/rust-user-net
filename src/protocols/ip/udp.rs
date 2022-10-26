use super::{
    get_interface, IPAdress, IPEndpoint, IPInterface, IPProtocolType, IP_ADDR_ANY,
    IP_PAYLOAD_MAX_SIZE,
};
use crate::{
    devices::NetDevice,
    protocol_stack::ProtocolContexts,
    util::{bytes_to_struct, cksum16, le_to_be_u16, to_u8_slice},
};
use std::{
    collections::VecDeque,
    mem::size_of,
    sync::mpsc::{self, Sender},
};

const UDP_PCB_COUNT: usize = 16;
const UDP_SRC_PORT_MIN: u16 = 49152;
const UDP_SRC_PORT_MAX: u16 = 65535;

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
pub struct UdpPcb {
    state: UdpPcbState,
    local_endpoint: IPEndpoint,
    sender: Option<Sender<()>>,
    data_entries: VecDeque<UdpDataEntry>,
}

impl UdpPcb {
    pub fn new() -> UdpPcb {
        UdpPcb {
            state: UdpPcbState::Free,
            local_endpoint: IPEndpoint {
                address: IP_ADDR_ANY,
                port: 0,
            },
            sender: None,
            data_entries: VecDeque::new(),
        }
    }
}

struct UdpDataEntry {
    remote_endpoint: IPEndpoint,
    len: usize,
    data: Vec<u8>,
}

pub struct UdpPcbs {
    pub entries: Vec<UdpPcb>,
}

impl UdpPcbs {
    pub fn new() -> UdpPcbs {
        let mut entries = Vec::with_capacity(UDP_PCB_COUNT);
        for _ in 0..UDP_PCB_COUNT {
            entries.push(UdpPcb::new());
        }
        UdpPcbs { entries }
    }

    fn delete_entry(&mut self, pcb_id: usize) {
        let mut entry = &mut self.entries[pcb_id];

        entry.state = UdpPcbState::Closing;
        if entry.sender.is_some() {
            entry.sender.as_ref().unwrap().send(()).unwrap();
        }

        entry.state = UdpPcbState::Free;
        entry.local_endpoint.address = IP_ADDR_ANY;
        entry.local_endpoint.port = 0;
        entry.data_entries.clear();
    }

    pub fn get_by_id(&self, pcb_id: usize) -> Option<&UdpPcb> {
        self.entries.get(pcb_id)
    }

    pub fn get_mut_by_id(&mut self, pcb_id: usize) -> Option<&mut UdpPcb> {
        self.entries.get_mut(pcb_id)
    }

    pub fn get_by_host(&mut self, host_addr: IPAdress, host_port: u16) -> Option<&mut UdpPcb> {
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

    pub fn is_endpoint_used(&self, host_addr: IPAdress, host_port: u16) -> bool {
        for pcb in self.entries.iter() {
            if pcb.state == UdpPcbState::Open {
                if (pcb.local_endpoint.address == IP_ADDR_ANY
                    || host_addr == IP_ADDR_ANY
                    || pcb.local_endpoint.address == host_addr)
                    && pcb.local_endpoint.port == host_port
                {
                    return true;
                }
            }
        }
        false
    }
}

pub fn input(
    data: &[u8],
    len: usize,
    src: IPAdress,
    dst: IPAdress,
    device: &mut NetDevice,
    iface: &IPInterface,
    contexts: &mut ProtocolContexts,
) -> Result<(), ()> {
    let udp_hdr_size = size_of::<UdpHeader>();
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

    let pcb_opt = contexts.udp_pcbs.get_by_host(dst, header.dst_port);
    if pcb_opt.is_none() {
        println!(
            "There is no connection for IP: {:?}:{:?}",
            dst, header.dst_port
        );
        return Err(());
    }

    println!(
        "UDP input: source port = {:?} destination port: {:?}",
        header.src_port, header.dst_port
    );

    let pcb = pcb_opt.unwrap();
    let udp_data = data[udp_hdr_size..].to_vec();
    let remote_endpoint = IPEndpoint {
        address: src, // packet source is remote address
        port: header.src_port,
    };
    let data_entry = UdpDataEntry {
        remote_endpoint,
        len: len - udp_hdr_size,
        data: udp_data,
    };
    pcb.data_entries.push_back(data_entry);

    let sender = pcb.sender.as_ref().unwrap();
    sender.send(()).unwrap();

    Ok(())
}

pub fn output(
    src: IPEndpoint,
    dst: IPEndpoint,
    mut udp_data: Vec<u8>,
    device: &mut NetDevice,
    contexts: &mut ProtocolContexts,
) {
    println!("UDP: output");
    let udp_hdr_size = size_of::<UdpHeader>();
    let len = udp_data.len();
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
        contexts,
    )
    .unwrap();
}

// Public APIs

pub fn open(pcbs: &mut UdpPcbs) -> usize {
    for (i, entry) in pcbs.entries.iter_mut().enumerate() {
        if entry.state == UdpPcbState::Free {
            entry.state = UdpPcbState::Open;
            return i;
        }
    }
    panic!("UDP: there's no open PCB entry.");
}

pub fn bind(pcbs: &mut UdpPcbs, pcb_id: usize, local_endpoint: IPEndpoint) {
    let existing = pcbs.get_by_host(local_endpoint.address, local_endpoint.port);
    if existing.is_some() {
        panic!(
            "UDP: IP address {:?} & port {:?} is already in use.",
            local_endpoint.address, local_endpoint.port
        );
    }
    for (i, entry) in pcbs.entries.iter_mut().enumerate() {
        if pcb_id == i {
            entry.local_endpoint = local_endpoint;
            return;
        }
    }
    panic!("UDP: no PCB entry with specified id: {pcb_id}.");
}

pub fn send_to(
    pcb_id: usize,
    data: Vec<u8>,
    remote: IPEndpoint,
    device: &mut NetDevice,
    contexts: &mut ProtocolContexts,
) {
    let pcb = contexts
        .udp_pcbs
        .get_by_id(pcb_id)
        .expect("UDP (receive_from): no specified PCB entry.");

    // Local address setup in case not set in PCB
    let mut local_endpoint = IPEndpoint::new(pcb.local_endpoint.address, 0);
    if local_endpoint.address == IP_ADDR_ANY {
        let interface = get_interface(&contexts.ip_routes, remote.address)
            .expect("UDP: interface not found for remote address.");
        local_endpoint.address = interface.unicast;
    }
    // Local port setup in case not set in PCB
    if pcb.local_endpoint.port == 0 {
        for p in UDP_SRC_PORT_MIN..UDP_SRC_PORT_MAX {
            let is_used = contexts
                .udp_pcbs
                .is_endpoint_used(local_endpoint.address, p);
            if is_used == false {
                println!("UDP: assigned a port number: {p}");
                local_endpoint.port = p;
                break;
            }
        }
        if local_endpoint.port == 0 {
            panic!("UDP: failed to dynamically assign port.")
        }
    }

    output(local_endpoint, remote, data, device, contexts)
}

pub fn receive_from(pcbs: &mut UdpPcbs, pcb_id: usize) -> Option<Vec<u8>> {
    let pcb = pcbs
        .get_mut_by_id(pcb_id)
        .expect("UDP(receive_from): no specified PCB entry.");

    let (sender, receiver) = mpsc::channel();
    pcb.sender = Some(sender);

    loop {
        receiver.recv().unwrap();
        if pcb.state != UdpPcbState::Open {
            println!("UDP(receive_from): PCB got closed.");
            return None;
        }
        let data_entry = pcb.data_entries.pop_front();
        if data_entry.is_some() {
            let entry = data_entry.unwrap();
            return Some(entry.data);
        }
    }
}
