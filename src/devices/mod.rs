pub mod ethernet;
pub mod loopback;

use crate::{
    interrupt,
    net::IPInterface,
    protocols::{NetProtocol, ProtocolType},
};
use signal_hook::{consts::SIGUSR1, low_level::raise};
use std::sync::Arc;

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
    mtu: u16,
    flags: u16,
    header_len: u16,
    address_len: u16,
    pub address: [u8; 14],
    pub irq_entry: interrupt::IRQEntry,
    pub interface: Option<Box<IPInterface>>,
    pub next_device: Option<Box<NetDevice>>,
}

impl NetDevice {
    pub fn new(
        device_type: NetDeviceType,
        name: String,
        mtu: u16,
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
        tr_type: ProtocolType,
        data: Arc<[u8]>,
        len: usize,
    ) -> Result<(), ()> {
        if !self.is_open() {
            panic!("Device is not open.")
        }
        match self.device_type {
            NetDeviceType::Loopback => loopback::transmit(self, data),
            NetDeviceType::Ethernet => ethernet::transmit(self, data),
        }
    }

    /// ISR (interrupt service routine) for registered IRQs. Handles inputs and raises SIGUSR1.
    pub fn isr(&self, _irq: i32, protocols: Option<&mut Box<NetProtocol>>) {
        match self.device_type {
            NetDeviceType::Loopback => {
                loopback::isr(self, protocols);
            }
            NetDeviceType::Ethernet => {
                ethernet::isr(self, protocols);
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
