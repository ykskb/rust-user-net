pub mod ethernet;
pub mod loopback;

use crate::{
    drivers::{DriverData, DriverType},
    interrupt,
    net::NetInterfaceFamily,
    protocols::{ip::IPInterface, NetProtocol, ProtocolData, ProtocolType},
    util::List,
};
use signal_hook::{consts::SIGUSR1, low_level::raise};
use std::sync::Arc;

use self::ethernet::ETH_ADDR_LEN;

const DEVICE_FLAG_UP: u16 = 0x0001;
pub const DEVICE_FLAG_LOOPBACK: u16 = 0x0010;
pub const DEVICE_FLAG_BROADCAST: u16 = 0x0020;
pub const DEVICE_FLAG_P2P: u16 = 0x0040;
pub const DEVICE_FLAG_NEED_ARP: u16 = 0x0100;

pub const IRQ_FLAG_SHARED: u8 = 0x0001;
pub const NET_DEVICE_ADDR_LEN: usize = 14;

#[derive(Debug, PartialEq)]
pub enum NetDeviceType {
    Loopback,
    Ethernet,
}

pub struct NetDevice {
    index: u8,
    pub device_type: NetDeviceType,
    pub name: String,
    pub mtu: usize,
    pub flags: u16,
    pub header_len: u16,
    pub address_len: u16,
    pub address: [u8; NET_DEVICE_ADDR_LEN],
    pub broadcast: [u8; NET_DEVICE_ADDR_LEN],
    pub irq_entry: interrupt::IRQEntry,
    pub interfaces: List<Arc<IPInterface>>,
    pub driver_type: Option<DriverType>,
    pub driver_data: Option<DriverData>,
}

impl NetDevice {
    pub fn new(
        i: u8,
        device_type: NetDeviceType,
        name: String,
        mtu: usize,
        flags: u16,
        header_len: u16,
        address_len: u16,
        irq_entry: interrupt::IRQEntry,
    ) -> NetDevice {
        NetDevice {
            index: i,
            device_type,
            name,
            mtu,
            flags,
            header_len,
            address_len,
            address: [0; NET_DEVICE_ADDR_LEN],
            broadcast: [0; NET_DEVICE_ADDR_LEN],
            irq_entry,
            interfaces: List::<Arc<IPInterface>>::new(),
            driver_type: None,
            driver_data: None,
        }
    }

    pub fn register_interface(&mut self, interface: Arc<IPInterface>) {
        println!(
            "Registering {:?} interface on device: {}\n",
            interface.interface.family, self.name
        );
        // TODO: check duplicate inteface family type (IP or IPv6)
        self.interfaces.push(interface);
    }

    pub fn get_interface(&self, family: NetInterfaceFamily) -> Option<Arc<IPInterface>> {
        for ip_iface in self.interfaces.iter() {
            if ip_iface.interface.family == family {
                return Some(ip_iface.clone());
            }
        }
        None
    }

    fn is_open(&self) -> bool {
        self.flags & DEVICE_FLAG_UP > 0
    }

    pub fn open(&mut self) -> Result<(), ()> {
        self.flags |= DEVICE_FLAG_UP;
        match self.device_type {
            NetDeviceType::Loopback => loopback::open(self),
            NetDeviceType::Ethernet => ethernet::open(self),
        }
    }

    pub fn close(&self) -> Result<(), &str> {
        match self.device_type {
            NetDeviceType::Loopback => Ok(()),
            NetDeviceType::Ethernet => Ok(()),
        }
    }

    /// Sends data to a device.
    pub fn transmit(
        &mut self,
        proto_type: ProtocolType,
        data: Vec<u8>,
        len: usize,
        dst: [u8; ETH_ADDR_LEN],
    ) -> Result<(), ()> {
        if !self.is_open() {
            panic!("Device is not open.")
        }
        match self.device_type {
            NetDeviceType::Loopback => loopback::transmit(self, data),
            NetDeviceType::Ethernet => ethernet::transmit(self, proto_type, data, len, dst),
        }
    }

    /// ISR (interrupt service routine) for registered IRQs. Handles inputs and raises SIGUSR1.
    pub fn isr(&self, irq: i32, protocols: &mut List<NetProtocol>) {
        let incoming_data = match self.device_type {
            NetDeviceType::Loopback => loopback::read_data(self),
            NetDeviceType::Ethernet => ethernet::read_data(self),
        };

        if incoming_data.is_none() {
            return;
        }

        let (proto_type, data, len) = incoming_data.unwrap();
        for protocol in protocols.iter_mut() {
            if protocol.protocol_type == proto_type {
                let data_entry: ProtocolData = ProtocolData::new(irq, Some(Arc::new(data)), len);
                protocol.input_head.push_back(data_entry);
                break;
            }
        }

        raise(SIGUSR1).unwrap();
    }
}
