use super::{NetDevice, NetDeviceType};
use crate::{drivers::tap::open_tap, interrupt::IRQEntry, protocols::NetProtocol};
use std::sync::Arc;

pub enum EthDriverType {
    Tap,
    Pcap,
}

pub fn open(device: &mut NetDevice) -> Result<(), ()> {
    open_tap(device);
    Ok(())
}

pub fn isr(device: &NetDevice, protocols: Option<&mut Box<NetProtocol>>) {}

pub fn transmit(device: &mut NetDevice, data: Arc<[u8]>) -> Result<(), ()> {
    Ok(())
}

pub fn init(i: u8) -> NetDevice {
    let irq_entry = IRQEntry::new(0, 0);
    NetDevice::new(
        NetDeviceType::Ethernet,
        String::from("lo"),
        0,
        0,
        i,
        irq_entry,
    )
}
