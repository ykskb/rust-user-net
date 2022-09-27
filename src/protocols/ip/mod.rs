pub mod icmp;

use crate::{
    net::{IPAdress, NetInterface},
    util::{cksum16, le_to_be_u16},
};
use std::{
    mem::{size_of, size_of_val},
    sync::Mutex,
};

pub const IP_ADDR_LEN: usize = 4;

// see https://www.iana.org/assignments/protocol-numbers/protocol-numbers.txt
pub enum IPProtocolType {
    ICMP = 0x01,
    TCP = 0x06,
    UDP = 0x11,
}

pub struct IPProtocol {
    name: [char; 16],
    ip_type: IPProtocolType,
    next: Option<Box<IPProtocol>>,
    handler: fn(),
}

pub struct IPRoute {
    network: IPAdress,
    netmask: IPAdress,
    next_hop: IPAdress,
    interface: IPInterface,
    next: Option<Box<IPRoute>>,
}

#[derive(Debug)]
pub struct IPInterface {
    pub interface: NetInterface,
    pub next: Option<Box<IPInterface>>,
    pub unicast: IPAdress,
    pub netmask: IPAdress,
    pub broadcast: IPAdress,
}

const IP_VERSION_4: u8 = 4;

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

pub fn ip_device_output() {}

pub struct IPHeaderIdGenerator {
    id_mtx: Mutex<u16>,
}

impl IPHeaderIdGenerator {
    pub fn new() -> IPHeaderIdGenerator {
        IPHeaderIdGenerator {
            id_mtx: Mutex::new(128),
        }
    }

    pub fn generate_id(&mut self) -> u16 {
        let mut id = self.id_mtx.lock().unwrap();
        *id += 1;
        *id
    }
}

pub fn ip_output(ip_proto: IPProtocolType, data: Vec<u8>, src: IPAdress, dst: IPAdress) {
    let data: &[u8] = data.as_ref();
    let hlen = size_of::<IPHeader>();
    let len = size_of_val(&data);
    let total = hlen as u16 + len as u16;
    let mut id_manager = IPHeaderIdGenerator::new();
    let mut hdr = IPHeader {
        ver_len: (IP_VERSION_4 << 4) | (hlen as u8 >> 2),
        service_type: 0,
        total_len: le_to_be_u16(total),
        id: le_to_be_u16(id_manager.generate_id()),
        offset: 0,
        ttl: 0xff,
        protocol: IPProtocolType::ICMP as u8,
        check_sum: 0,
        src,
        dst,
        opts: [],
    };
    hdr.check_sum = cksum16(&hdr, hlen, 0);
}

#[cfg(test)]
mod test {
    use std::mem::{size_of, size_of_val};

    use crate::{
        net::ip_addr_to_bytes,
        util::{cksum16, le_to_be_u16},
    };

    use super::{IPHeader, IPHeaderIdGenerator, IPProtocolType, IP_VERSION_4};

    #[test]
    fn test_ip_header() {
        let data: [u8; 4] = [0x01, 0x02, 0x03, 0x04];
        let hlen = size_of::<IPHeader>();
        let len = size_of_val(&data);
        let total = hlen as u16 + len as u16;
        let mut id_manager = IPHeaderIdGenerator::new();
        let id = id_manager.generate_id();

        let hdr = IPHeader {
            ver_len: (IP_VERSION_4 << 4) | (hlen as u8 >> 2), // devide by 4
            service_type: 0,
            total_len: le_to_be_u16(total),
            id: le_to_be_u16(id),
            offset: 0,
            ttl: 0xff,
            protocol: IPProtocolType::ICMP as u8,
            check_sum: 0,
            src: ip_addr_to_bytes("192.0.0.1").unwrap(),
            dst: ip_addr_to_bytes("54.0.2.121").unwrap(),
            opts: [],
        };
        let res = cksum16(&hdr, hlen, 0);
        assert_eq!(0xC2E9, res);
    }
}
