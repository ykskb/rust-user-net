use super::{NetDevice, NetDeviceType, IRQ_FLAG_SHARED};
use crate::{interrupt, protocols::ProtocolType};
use signal_hook::low_level::raise;
use std::sync::Arc;

pub const IRQ_LOOPBACK: i32 = interrupt::INTR_IRQ_BASE + 5;
const LOOPBACK_MTU: usize = u16::MAX as usize;

pub fn open(_device: &mut NetDevice) -> Result<(), ()> {
    Ok(())
}

pub fn read_data(device: &NetDevice) -> Option<(ProtocolType, Vec<u8>, usize)> {
    let data = device.irq_entry.custom_data.as_ref().unwrap();
    Some((ProtocolType::IP, data.clone().as_ref().to_vec(), data.len()))
}

pub fn transmit(device: &mut NetDevice, data: Vec<u8>) -> Result<(), ()> {
    println!("Transmitting data through loopback device...\n");
    device.irq_entry.custom_data = Some(Arc::new(data));
    raise(IRQ_LOOPBACK).unwrap();
    Ok(())
}

pub fn init(i: u8) -> NetDevice {
    let irq_entry = interrupt::IRQEntry::new(IRQ_LOOPBACK, IRQ_FLAG_SHARED);
    NetDevice::new(
        i,
        NetDeviceType::Loopback,
        String::from("lo"),
        LOOPBACK_MTU,
        super::DEVICE_FLAG_LOOPBACK,
        0,
        0,
        irq_entry,
    )
}
