use crate::{
    protocols::{
        arp::ArpTable,
        ip::{tcp::TcpPcbs, udp::UdpPcbs, IPHeaderIdManager, IPRoute},
    },
    util::List,
};

#[derive(PartialEq, Debug)]
pub enum NetInterfaceFamily {
    IP,
    IPV6,
}

#[derive(Debug)]
pub struct NetInterface {
    pub family: NetInterfaceFamily,
    pub next: Option<Box<NetInterface>>,
}
