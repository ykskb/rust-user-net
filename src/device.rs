use crate::interrupt;
use signal_hook::low_level::raise;

static DEVICE_FLAG_UP: u16 = 0x0001;

enum NetDeviceType {
    Loopback,
    Ethernet,
    Nil,
}

pub struct NetDevice {
    index: u8,
    device_type: NetDeviceType,
    name: String,
    mtu: u16,
    flags: u16,
    header_len: u16,
    address_len: u16,
    pub next_device: Option<Box<NetDevice>>,
}

impl NetDevice {
    pub fn new(i: u8) -> NetDevice {
        NetDevice {
            index: i,
            device_type: NetDeviceType::Nil,
            name: String::from(""),
            mtu: 0,
            flags: 0,
            header_len: 0,
            address_len: 0,
            next_device: None,
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
            NetDeviceType::Nil => {
                self.flags |= DEVICE_FLAG_UP;
                Ok(())
            }
        }
    }
    pub fn close(&self) -> Result<(), &str> {
        match self.device_type {
            NetDeviceType::Loopback => Ok(()),
            NetDeviceType::Ethernet => Ok(()),
            NetDeviceType::Nil => Ok(()),
        }
    }
    pub fn transmit(&self, tr_type: u16, data: Box<&str>, len: usize) -> Result<(), ()> {
        if !self.is_open() {
            panic!("Device is not open.")
        }
        match self.device_type {
            NetDeviceType::Loopback => Ok(()),
            NetDeviceType::Ethernet => Ok(()),
            NetDeviceType::Nil => {
                println!("NIL TYPE DEVICE: data transmitted: {}\n", data);
                println!("Raising 36\n");
                raise(interrupt::INTR_IRQ_NULL).unwrap();
                Ok(())
            }
        }
    }
}
