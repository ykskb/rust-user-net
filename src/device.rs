use crate::interrupt;
use signal_hook::low_level::raise;

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
    irq_entry: interrupt::IRQEntry,
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
                next_device: None,
            },
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
    pub fn transmit(&self, tr_type: u16, data: &str, len: usize) -> Result<(), ()> {
        if !self.is_open() {
            panic!("Device is not open.")
        }
        match self.device_type {
            NetDeviceType::Loopback => {
                println!("NIL TYPE DEVICE: data transmitted: {}\n", data);
                println!("Raising 36\n");
                raise(IRQ_LOOPBACK).unwrap();
                Ok(())
            }
            NetDeviceType::Ethernet => Ok(()),
        }
    }
    pub fn isr(&self, irq: u8, device: &NetDevice) {}
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
