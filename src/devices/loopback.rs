use super::{NetDevice, NetDeviceType, IRQ_FLAG_SHARED};
use crate::{
    interrupt,
    protocols::{NetProtocol, ProtocolData, ProtocolType},
};
use signal_hook::low_level::raise;
use std::sync::Arc;

pub const IRQ_LOOPBACK: i32 = interrupt::INTR_IRQ_BASE + 5;
const DEVICE_FLAG_LOOPBACK: u16 = 0x0010;
const LOOPBACK_MTU: u16 = u16::MAX;

pub fn open(_device: &mut NetDevice) -> Result<(), ()> {
    Ok(())
}

pub fn read_data(device: &NetDevice) -> Option<(ProtocolType, Vec<u8>)> {
    Some((
        ProtocolType::IP,
        device
            .irq_entry
            .custom_data
            .as_ref()
            .unwrap()
            .clone()
            .as_ref()
            .to_vec(),
    ))
}

pub fn transmit(device: &mut NetDevice, data: Arc<Vec<u8>>) -> Result<(), ()> {
    println!("Transmitting data through loopback device...\n");
    device.irq_entry.custom_data = Some(data);
    raise(IRQ_LOOPBACK).unwrap();
    Ok(())
}

pub fn init(i: u8) -> NetDevice {
    let irq_entry = interrupt::IRQEntry::new(IRQ_LOOPBACK, IRQ_FLAG_SHARED);
    NetDevice::new(
        NetDeviceType::Loopback,
        String::from("lo"),
        LOOPBACK_MTU,
        DEVICE_FLAG_LOOPBACK,
        i,
        irq_entry,
    )
}
