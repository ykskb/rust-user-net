pub mod icmp;
pub mod tcp;
pub mod udp;

use super::arp::arp_resolve;
use crate::{
    devices::{ethernet::ETH_ADDR_LEN, NetDevice, DEVICE_FLAG_NEED_ARP},
    net::{NetInterface, NetInterfaceFamily},
    protocol_stack::ProtocolContexts,
    util::{be_to_le_u16, be_to_le_u32, bytes_to_struct, cksum16, le_to_be_u16, to_u8_slice, List},
};
use std::{
    convert::TryInto,
    mem::size_of,
    sync::{Arc, Mutex},
};

pub type IPAdress = u32;

pub const IP_ADDR_LEN: usize = 4;
const IP_MAX_SIZE: usize = u16::MAX as usize;
const IP_HEADER_MIN_SIZE: usize = 20;
const IP_PAYLOAD_MAX_SIZE: usize = IP_MAX_SIZE - IP_HEADER_MIN_SIZE;

const IP_VERSION_4: u8 = 4;

const IP_ADDR_ANY: IPAdress = 0x00000000; // 0.0.0.0
const IP_ADDR_BROADCAST: IPAdress = 0xffffffff; // 255.255.255.255

pub struct IPEndpoint {
    pub address: IPAdress,
    pub port: u16,
}

impl IPEndpoint {
    pub fn new(addr: &str, port: u16) -> IPEndpoint {
        IPEndpoint {
            address: ip_addr_to_bytes(addr).unwrap(),
            port: le_to_be_u16(port),
        }
    }
}

#[derive(Debug)]
pub struct IPInterface {
    pub interface: NetInterface,
    pub next: Option<Box<IPInterface>>,
    pub unicast: IPAdress,
    pub netmask: IPAdress,
    pub broadcast: IPAdress,
}

impl IPInterface {
    pub fn new(unicast: &str, netmask: &str) -> IPInterface {
        let interface = NetInterface {
            family: NetInterfaceFamily::IP,
            next: None,
        };
        let unicast = ip_addr_to_bytes(unicast).unwrap();
        let netmask = ip_addr_to_bytes(netmask).unwrap();
        // unicast & netmask = nw address => nw address | !nestmask (all hosts) = broadcast
        let broadcast = (unicast & netmask) | !netmask;

        IPInterface {
            interface,
            next: None,
            unicast,
            netmask,
            broadcast,
        }
    }
}

pub struct IPRoute {
    network: IPAdress,
    netmask: IPAdress,
    next_hop: IPAdress,
    interface: Arc<IPInterface>,
}

impl IPRoute {
    pub fn interface_route(interface: Arc<IPInterface>) -> IPRoute {
        IPRoute {
            network: interface.unicast & interface.netmask,
            netmask: interface.netmask,
            next_hop: IP_ADDR_ANY,
            interface,
        }
    }

    pub fn gateway_route(gateway_ip: &str, interface: Arc<IPInterface>) -> IPRoute {
        IPRoute {
            network: IP_ADDR_ANY,
            netmask: IP_ADDR_ANY,
            next_hop: ip_addr_to_bytes(gateway_ip).unwrap(),
            interface,
        }
    }
}

pub fn lookup_ip_route(routes: &List<IPRoute>, dst: IPAdress) -> Option<&IPRoute> {
    let mut candidate = None;
    for route in routes.iter() {
        if (dst & route.netmask) == route.network {
            if candidate.is_none() {
                candidate = Some(route);
            } else {
                let candidate_route = candidate.unwrap();
                if be_to_le_u32(candidate_route.netmask) < be_to_le_u32(route.netmask) {
                    candidate = Some(route);
                }
            }
        }
    }
    candidate
}

pub fn get_interface(routes: &List<IPRoute>, dst: IPAdress) -> Option<Arc<IPInterface>> {
    let route = lookup_ip_route(routes, dst);
    route?;
    Some(route.unwrap().interface.clone())
}

// see https://www.iana.org/assignments/protocol-numbers/protocol-numbers.txt
pub enum IPProtocolType {
    Icmp = 0x01,
    Tcp = 0x06,
    Udp = 0x11,
    Unknown,
}

impl IPProtocolType {
    pub fn from_u8(value: u8) -> IPProtocolType {
        match value {
            0x01 => IPProtocolType::Icmp,
            0x06 => IPProtocolType::Tcp,
            0x11 => IPProtocolType::Udp,
            _ => IPProtocolType::Unknown,
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
    opts: [u8; 0],
}

pub struct IPHeaderIdManager {
    id_mtx: Mutex<u16>,
}

impl IPHeaderIdManager {
    pub fn new() -> IPHeaderIdManager {
        IPHeaderIdManager {
            id_mtx: Mutex::new(128),
        }
    }

    pub fn generate_id(&mut self) -> u16 {
        let mut id = self.id_mtx.lock().unwrap();
        *id += 1;
        *id
    }
}

fn create_ip_header(
    ip_proto: IPProtocolType,
    src: IPAdress,
    dst: IPAdress,
    data: &Vec<u8>,
    id: u16,
) -> IPHeader {
    let hlen = size_of::<IPHeader>();
    let len = data.len();
    let total = hlen as u16 + len as u16;

    // TODO: check MTU vs header size + len

    let mut header = IPHeader {
        ver_len: (IP_VERSION_4 << 4) | (hlen as u8 >> 2),
        service_type: 0,
        total_len: le_to_be_u16(total),
        id: le_to_be_u16(id),
        offset: 0,
        ttl: 0xff,
        protocol: ip_proto as u8,
        check_sum: 0,
        src,
        dst,
        opts: [],
    };
    let header_bytes = unsafe { to_u8_slice(&header) };
    header.check_sum = le_to_be_u16(cksum16(header_bytes, hlen, 0));
    header
}

pub fn output(
    ip_proto: IPProtocolType,
    mut data: Vec<u8>,
    src: IPAdress,
    dst: IPAdress,
    device: &mut NetDevice,
    contexts: &mut ProtocolContexts,
) -> Result<(), ()> {
    let route_lookup = lookup_ip_route(&contexts.ip_routes, dst);
    if route_lookup.is_none() {
        return Err(());
    }
    let route = route_lookup.unwrap();

    if src != IP_ADDR_ANY && src != route.interface.unicast {
        println!(
            "Source address: {:?} not matching with interface unicast: {:?}",
            ip_addr_to_str(src),
            ip_addr_to_str(route.interface.unicast)
        );
        return Err(());
    }
    let next_hop = if route.next_hop != IP_ADDR_ANY {
        route.next_hop
    } else {
        dst
    };

    let header = create_ip_header(
        ip_proto,
        route.interface.unicast,
        dst,
        &data,
        contexts.ip_id_manager.generate_id(),
    );

    println!(
        "IP output: header destination = {:?} src = {:?} nexthop = {:?}",
        ip_addr_to_str(header.dst),
        ip_addr_to_str(header.src),
        ip_addr_to_str(next_hop)
    );

    let header_bytes = unsafe { to_u8_slice::<IPHeader>(&header) }; // add icmp data here
    let mut ip_data = header_bytes.to_vec();
    ip_data.append(&mut data);
    let ip_data_len = ip_data.len();

    let mut hw_addr: [u8; ETH_ADDR_LEN] = [0; ETH_ADDR_LEN];
    if device.flags & DEVICE_FLAG_NEED_ARP > 0 {
        if dst == route.interface.broadcast || dst == IP_ADDR_BROADCAST {
            hw_addr = device.broadcast[..ETH_ADDR_LEN + 1].try_into().unwrap();
        } else {
            let arp = arp_resolve(
                device,
                route.interface.clone(),
                &mut contexts.arp_table,
                next_hop,
            );
            if let Ok(result) = arp {
                if result.is_none() {
                    println!("Waiting for ARP reply...");
                    return Ok(());
                }
                hw_addr = result.unwrap();
            }
        }
    }

    device.transmit(super::ProtocolType::IP, ip_data, ip_data_len, hw_addr)
}

fn check_ip_header(header: &IPHeader, data_len: usize, header_len: usize) -> Result<(), ()> {
    let ip_version = header.ver_len >> 4;
    if ip_version != IP_VERSION_4 {
        println!("IP input: version error with value: {ip_version}");
        return Err(());
    }
    if data_len < header_len {
        println!("IP input: header length error.");
        return Err(());
    }
    if data_len < be_to_le_u16(header.total_len) as usize {
        println!("IP input: total length error.");
        return Err(());
    }
    let header_bytes = unsafe { to_u8_slice(header) };
    if cksum16(header_bytes, header_len, 0) != 0 {
        println!("IP input: checksum error.");
        return Err(());
    }
    let offset = be_to_le_u16(header.offset);
    if offset & 0x2000 > 0 || offset & 0x1fff > 0 {
        println!("IP input: fragment is not supported.");
        return Err(());
    }
    Ok(())
}

pub fn input(
    data: &[u8],
    len: usize,
    device: &mut NetDevice,
    contexts: &mut ProtocolContexts,
) -> Result<(), ()> {
    if len < IP_HEADER_MIN_SIZE {
        panic!("IP input: data is too short.")
    }
    let header = unsafe { bytes_to_struct::<IPHeader>(data) };
    let header_len = ((header.ver_len & 0x0f) << 2) as usize;
    if let Err(_e) = check_ip_header(&header, len, header_len) {
        return Err(());
    }
    println!(
        "IP input src: {:?} dst: {:?}",
        ip_addr_to_str(header.src),
        ip_addr_to_str(header.dst)
    );
    let interface_lookup = device.get_interface(NetInterfaceFamily::IP);
    if let Some(interface) = interface_lookup {
        if interface.unicast != header.dst {
            return Err(());
        }
        let sub_data = &data[header_len..];
        match IPProtocolType::from_u8(header.protocol) {
            IPProtocolType::Icmp => {
                return icmp::input(
                    sub_data,
                    len - header_len,
                    header.src,
                    header.dst,
                    device,
                    &interface,
                    contexts,
                );
            }
            IPProtocolType::Tcp => {
                return tcp::input();
            }
            IPProtocolType::Udp => {
                return udp::input(
                    sub_data,
                    len - header_len,
                    header.src,
                    header.dst,
                    device,
                    &interface,
                );
            }
            IPProtocolType::Unknown => {
                return Ok(());
            }
        };
    }
    Ok(())
}

/// Converts string IP to bytes in big endian.
pub fn ip_addr_to_bytes(addr: &str) -> Option<IPAdress> {
    let mut parts = addr.split('.');
    let mut part;
    let mut res: u32 = 0;
    for i in 0..4 {
        part = parts.next();
        part?;
        let b = part.unwrap().parse::<u8>().unwrap();
        res |= (b as u32) << (8 * i);
    }
    Some(res)
}

/// Converts IP bytes in big endian to string.
pub fn ip_addr_to_str(addr: IPAdress) -> String {
    let mut parts = Vec::new();
    for i in 0..4 {
        let d = (addr >> (8 * i)) & 255;
        parts.push(d.to_string());
    }
    parts.join(".")
}

#[cfg(test)]
mod tests {
    use super::{ip_addr_to_bytes, ip_addr_to_str};

    #[test]
    fn test_ip_addr_to_bytes() {
        let b = ip_addr_to_bytes("127.0.0.1");
        assert_eq!(0x0100007F, b.unwrap());
    }

    #[test]
    fn test_ip_addr_to_str() {
        let s = ip_addr_to_str(0x0100007F);
        assert_eq!("127.0.0.1", s);
    }
}

#[cfg(test)]
mod test {
    use std::mem::{size_of, size_of_val};

    use crate::{
        protocols::ip::ip_addr_to_bytes,
        util::{cksum16, le_to_be_u16, to_u8_slice},
    };

    use super::{IPHeader, IPHeaderIdManager, IPProtocolType, IP_VERSION_4};

    #[test]
    fn test_ip_header() {
        let data: [u8; 4] = [0x01, 0x02, 0x03, 0x04];
        let hlen = size_of::<IPHeader>();
        let len = size_of_val(&data);
        let total = hlen as u16 + len as u16;
        let mut id_manager = IPHeaderIdManager::new();
        let id = id_manager.generate_id();

        let hdr = IPHeader {
            ver_len: (IP_VERSION_4 << 4) | (hlen as u8 >> 2), // devide by 4
            service_type: 0,
            total_len: le_to_be_u16(total),
            id: le_to_be_u16(id),
            offset: 0,
            ttl: 0xff,
            protocol: IPProtocolType::Icmp as u8,
            check_sum: 0,
            src: ip_addr_to_bytes("192.0.0.1").unwrap(),
            dst: ip_addr_to_bytes("54.0.2.121").unwrap(),
            opts: [],
        };
        let header_bytes = unsafe { to_u8_slice(&hdr) };
        let res = cksum16(header_bytes, hlen, 0);
        assert_eq!(0xC2E9, res);
    }
}
