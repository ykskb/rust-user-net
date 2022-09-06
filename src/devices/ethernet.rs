use super::{NetDevice, NetDeviceType};
use crate::{
    drivers::{tap, DriverData, DriverType},
    interrupt::IRQEntry,
    protocols::{NetProtocol, ProtocolType},
};
use std::sync::Arc;

pub const ETH_FRAME_MAX: usize = 1514;
pub const ETH_ADDR_ANY: [u8; 6] = [0x00; 6];
pub const ETH_ADDR_BROADCAST: [u8; 6] = [0xff; 6];
pub const ETH_ADDR_LEN: usize = 6;

#[repr(packed)]
pub struct EthernetHeader {
    pub dst: [u8; ETH_ADDR_LEN],
    pub src: [u8; ETH_ADDR_LEN],
    pub eth_type: u16,
}

pub fn open(device: &mut NetDevice) -> Result<(), ()> {
    match device.driver_type.as_ref().unwrap() {
        DriverType::Tap => {
            tap::open(device);
        }
        DriverType::Pcap => {}
    }
    Ok(())
}

pub fn read_data(device: &NetDevice) -> Option<(ProtocolType, Vec<u8>)> {
    match device.driver_type.as_ref().unwrap() {
        DriverType::Tap => tap::read_data(device),
        DriverType::Pcap => None,
    }
}

pub fn transmit(device: &mut NetDevice, data: Arc<Vec<u8>>) -> Result<(), ()> {
    Ok(())
}

pub fn init(i: u8, driver_type: DriverType) -> NetDevice {
    let irq_entry = IRQEntry::new(0, 0);
    let mut device = NetDevice::new(
        NetDeviceType::Ethernet,
        String::from("lo"),
        0,
        0,
        i,
        irq_entry,
    );
    device.driver_type = Some(driver_type);
    device
}
