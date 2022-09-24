pub mod ethernet;
pub mod loopback;

use crate::{
    drivers::{DriverData, DriverType},
    interrupt,
    net::IPInterface,
    protocols::{NetProtocol, ProtocolData, ProtocolType},
    util::List,
};
use signal_hook::{consts::SIGUSR1, low_level::raise};
use std::sync::Arc;

use self::ethernet::ETH_ADDR_LEN;

const DEVICE_FLAG_UP: u16 = 0x0001;

pub const IRQ_FLAG_SHARED: u8 = 0x0001;

#[derive(Debug)]
pub enum NetDeviceType {
    Loopback,
    Ethernet,
}

#[derive(Debug)]
pub struct NetDevice {
    index: u8,
    device_type: NetDeviceType,
    pub name: String,
    mtu: usize,
    flags: u16,
    header_len: u16,
    address_len: u16,
    pub address: [u8; 14],
    pub irq_entry: interrupt::IRQEntry,
    pub interface: Option<Box<IPInterface>>,
    pub next_device: Option<Box<NetDevice>>,
    pub driver_type: Option<DriverType>,
    pub driver_data: Option<DriverData>,
}

impl NetDevice {
    pub fn new(
        device_type: NetDeviceType,
        name: String,
        mtu: usize,
        flags: u16,
        i: u8,
        irq_entry: interrupt::IRQEntry,
    ) -> NetDevice {
        NetDevice {
            index: i,
            device_type,
            name,
            mtu,
            flags,
            header_len: 0,
            address_len: 0,
            address: [0; 14],
            irq_entry,
            interface: None,
            next_device: None,
            driver_type: None,
            driver_data: None,
        }
    }

    pub fn register_interface(&mut self, interface: IPInterface) {
        println!(
            "Registering {:?} interface on device: {}\n",
            interface.interface.family, self.name
        );
        let interface = Box::new(interface);
        if self.interface.is_none() {
            self.interface = Some(interface);
        } else {
            let mut head = self.interface.as_mut().unwrap();
            while head.next.is_some() {
                head = head.next.as_mut().unwrap();
            }
            head.next = Some(interface);
        }
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

    pub fn open_all(&mut self) -> Result<(), &str> {
        self.open().unwrap();
        let mut head = &mut self.next_device;
        while head.is_some() {
            let dev = head.as_mut().unwrap();
            let d = dev.as_mut();
            d.open().unwrap();
            head = &mut d.next_device;
        }
        Ok(())
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
        data: Arc<Vec<u8>>,
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

        let (proto_type, data) = incoming_data.unwrap();
        for protocol in protocols.iter_mut() {
            if protocol.protocol_type == proto_type {
                let data_entry: ProtocolData = ProtocolData::new(Some(Arc::new(data)), irq);
                protocol.input_head.push_back(data_entry);
                break;
            }
        }

        raise(SIGUSR1).unwrap();
    }
}

// fn add_device(device: NetDevice, new_device: NetDevice) {
//     let mut head = &mut device;
//     while head.next_device.is_some() {
//         head = &mut head.next_device.unwrap();
//     }
//     head.next_device = Some(Box::new(new_device));
// }
