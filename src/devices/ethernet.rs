use super::{NetDevice, NetDeviceType};
use crate::{
    drivers::{tap, DriverType},
    interrupt::IRQEntry,
    protocols::ProtocolType,
    util::{le_to_be_u16, to_u8_slice},
};
use std::{convert::TryInto, sync::Arc};

const ETH_HDR_SIZE: usize = 14;
const ETH_FRAME_MIN: usize = 60; // without FCS
pub const ETH_FRAME_MAX: usize = 1514; // without FCS
const ETH_PAYLOAD_MIN: usize = ETH_FRAME_MIN - ETH_HDR_SIZE;
const ETH_PAYLOAD_MAX: usize = ETH_FRAME_MAX - ETH_HDR_SIZE;

pub const ETH_ADDR_ANY: [u8; 6] = [0x00; 6];
pub const ETH_ADDR_BROADCAST: [u8; 6] = [0xff; 6];
pub const ETH_ADDR_LEN: usize = 6;

/// Ethernet Header (unit: octet)
/// [ Preamble: 7 | SDF: 1 | Dst MAC: 6 | Src MAC: 6 | EtherType: 2 | Payload: to 1500 | FCS: 4 ]
/// SFD: start frame delimiter / FCS: frame check sequence (32bit-CRC)
///
/// EtherType in Ethernet II:
/// 0x0800: IPv4 | 0x0806: ARP | 0x86DD: IPv6
///
/// MAC Address
/// [ OUI: 3 | Product ID: 3 ]
/// b0: 0: unicast | 1: broadcast
/// b1: 0: global address | 1: local address
#[repr(packed)]
pub struct EthernetHeader {
    pub dst: [u8; ETH_ADDR_LEN], // destination MAC: 6 octets
    pub src: [u8; ETH_ADDR_LEN], // source MAC: 6 octets
    pub eth_type: u16,           // ethernet type : 2 octets IEEE 802.3
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

pub fn transmit(
    device: &mut NetDevice,
    ether_type: ProtocolType,
    data: Vec<u8>,
    len: usize,
    dst: [u8; ETH_ADDR_LEN],
) -> Result<(), ()> {
    let src_address: [u8; 6] = device.address[..ETH_ADDR_LEN]
        .try_into()
        .expect("Device address size error.");

    let hdr = EthernetHeader {
        dst,
        src: src_address,
        eth_type: le_to_be_u16(ether_type as u16),
    };
    let hdr_bytes = unsafe { to_u8_slice::<EthernetHeader>(&hdr) };

    let mut frame: [u8; ETH_FRAME_MAX] = [0; ETH_FRAME_MAX];
    let mut pad_len: usize = 0;
    let data_len = data.len();
    let hdr_len = hdr_bytes.len();

    frame[..hdr_len].copy_from_slice(hdr_bytes);
    frame[hdr_len..(hdr_len + data_len)].copy_from_slice(&data[..]);

    if data_len < ETH_PAYLOAD_MIN {
        pad_len = ETH_PAYLOAD_MIN - data_len;
    }
    let frame_len = hdr_len + data_len + pad_len;

    match device.driver_type.as_ref().unwrap() {
        DriverType::Tap => tap::write_data(device, &frame[..frame_len]),
        DriverType::Pcap => Ok(()),
    }
}

pub fn init(i: u8, driver_type: DriverType) -> NetDevice {
    let irq_entry = IRQEntry::new(0, 0);
    let mut device = NetDevice::new(
        NetDeviceType::Ethernet,
        String::from("lo"),
        ETH_PAYLOAD_MAX,
        0,
        i,
        irq_entry,
    );
    device.driver_type = Some(driver_type);
    device
}
