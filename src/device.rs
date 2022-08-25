use std::{rc::Rc, sync::Arc};

use crate::{
    interrupt,
    net::{IPInterface, NetInterface},
    protocol::{NetProtocol, ProtocolData, ProtocolType},
};
use signal_hook::{consts::SIGUSR1, low_level::raise};

const DEVICE_FLAG_UP: u16 = 0x0001;
const DEVICE_FLAG_LOOPBACK: u16 = 0x0010;
const LOOPBACK_MTU: u16 = u16::MAX;

pub const IRQ_LOOPBACK: i32 = interrupt::INTR_IRQ_BASE + 5;
const IRQ_FLAG_SHARED: u8 = 0x0001;

#[derive(Debug)]
pub enum NetDeviceType {
    Loopback,
    Ethernet,
}

#[derive(Debug)]
pub struct NetDevice {
    index: u8,
    device_type: NetDeviceType,
    name: String,
    mtu: u16,
    flags: u16,
    header_len: u16,
    address_len: u16,
    pub irq_entry: interrupt::IRQEntry,
    pub interface: Option<Box<IPInterface>>,
    pub next_device: Option<Box<NetDevice>>,
}

impl NetDevice {
    pub fn new(device_type: NetDeviceType, i: u8, irq_entry: interrupt::IRQEntry) -> NetDevice {
        match device_type {
            NetDeviceType::Loopback => NetDevice {
                index: i,
                device_type,
                name: String::from("lo"),
                mtu: LOOPBACK_MTU,
                flags: DEVICE_FLAG_LOOPBACK,
                header_len: 0,
                address_len: 0,
                irq_entry,
                interface: None,
                next_device: None,
            },
            NetDeviceType::Ethernet => NetDevice {
                index: i,
                device_type,
                name: String::from("eth0"),
                mtu: 0,
                flags: 0,
                header_len: 0,
                address_len: 0,
                irq_entry,
                interface: None,
                next_device: None,
            },
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

    pub fn open(&mut self) -> Result<(), &str> {
        match self.device_type {
            NetDeviceType::Loopback => {
                self.flags |= DEVICE_FLAG_UP;
                Ok(())
            }
            NetDeviceType::Ethernet => {
                self.flags |= DEVICE_FLAG_UP;
                Ok(())
            }
        }
    }

    pub fn open_all(&mut self) -> Result<(), &str> {
        self.open();
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
            NetDeviceType::Loopback => {
                println!("Transmitting data through loopback device...\n");
                self.irq_entry.custom_data = Some(data);
                raise(IRQ_LOOPBACK).unwrap();
                Ok(())
            }
            NetDeviceType::Ethernet => Ok(()),
        }
    }

    /// ISR (interrupt service routine) for registered IRQs. Handles inputs and raises SIGUSR1.
    pub fn isr(&self, _irq: i32, protocols: Option<&mut Box<NetProtocol>>) {
        match self.device_type {
            NetDeviceType::Loopback => {
                let mut head = protocols;
                while head.is_some() {
                    println!("Loopback device pushing data into protocol queue.\n");
                    let protocol = head.unwrap();
                    if protocol.protocol_type == ProtocolType::IP {
                        let custom_data_arc = self.irq_entry.custom_data.clone();
                        let data_entry: ProtocolData = ProtocolData::new(custom_data_arc);
                        protocol.input_head.push_back(data_entry);
                    }
                    head = protocol.next_protocol.as_mut();
                }
            }
            NetDeviceType::Ethernet => (),
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

pub fn init_loopback() -> NetDevice {
    let loopback_irq = interrupt::IRQEntry::new(IRQ_LOOPBACK, IRQ_FLAG_SHARED);
    let loopback_device: NetDevice = NetDevice::new(NetDeviceType::Loopback, 0, loopback_irq);
    loopback_device
}
