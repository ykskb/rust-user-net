use std::char;

#[derive(Debug)]
enum NetInterfaceFamily {
    IP,
    IPV6,
}
#[derive(Debug)]
pub struct NetInterface {
    family: NetInterfaceFamily,
    next: Option<Box<NetInterface>>,
}

pub type IPAdress = u32;

#[derive(Debug)]
pub struct IPInterface {
    interface: NetInterface,
    next: Option<Box<IPInterface>>,
    unicast: IPAdress,
    netmask: IPAdress,
    broadcast: IPAdress,
}

pub fn ip_addr_to_bytes(addr: &str) -> Option<IPAdress> {
    let mut parts = addr.split('.');
    let mut part;
    let mut res: u32 = 0;
    for i in (0..4).rev() {
        part = parts.next();
        part?;
        let b = part.unwrap().parse::<u8>().unwrap();
        res |= (b as u32) << (8 * i);
    }
    Some(res)
}

pub fn ip_addr_to_str(addr: IPAdress) -> String {
    let mut parts = Vec::new();
    for i in (0..4).rev() {
        let d = (addr >> (8 * i)) & 255;
        parts.push(d.to_string());
    }
    parts.join(".")
}

impl IPInterface {
    pub fn new(unicast: &str, netmask: &str) -> IPInterface {
        let interface = NetInterface {
            family: NetInterfaceFamily::IP,
            next: None,
        };
        IPInterface {
            interface,
            next: None,
            unicast: 3,
            netmask: 3,
            broadcast: 33,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ip_addr_to_bytes, ip_addr_to_str};

    #[test]
    fn test_ip_addr_to_bytes() {
        let b = ip_addr_to_bytes("127.0.0.1");
        assert_eq!(0x7F000001, b.unwrap());
    }

    #[test]
    fn test_ip_addr_to_str() {
        let s = ip_addr_to_str(0x7F000001);
        assert_eq!("127.0.0.1", s);
    }
}
