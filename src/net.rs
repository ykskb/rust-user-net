pub type IPAdress = u32;

#[derive(PartialEq, Debug)]
pub enum NetInterfaceFamily {
    IP,
    IPV6,
}

#[derive(Debug)]
pub struct NetInterface {
    pub family: NetInterfaceFamily,
    next: Option<Box<NetInterface>>,
}

#[derive(Debug)]
pub struct IPInterface {
    pub interface: NetInterface,
    pub next: Option<Box<IPInterface>>,
    pub unicast: IPAdress,
    pub netmask: IPAdress,
    pub broadcast: IPAdress,
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
