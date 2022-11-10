use super::{IPAdress, IPInterface, IPProtocolType};
use crate::{
    devices::NetDevice,
    protocols::ip::{ControlBlocks, ProtocolContexts},
    util::{bytes_to_struct, cksum16, to_u8_slice},
};
use std::mem::size_of;

const ICMP_TYPE_ECHOREPLY: u8 = 0;
const ICMP_TYPE_ECHO: u8 = 8;

// const ICMP_TYPE_DEST_UNREACH: u8 = 3;
// const ICMP_TYPE_SOURCE_QUENCH: u8 = 4;
// const ICMP_TYPE_REDIRECT: u8 = 5;
// const ICMP_TYPE_TIME_EXCEEDED: u8 = 11;
// const ICMP_TYPE_PARAM_PROBLEM: u8 = 12;
// const ICMP_TYPE_TIMESTAMP: u8 = 13;
// const ICMP_TYPE_TIMESTAMPREPLY: u8 = 14;
// const ICMP_TYPE_INFO_REQUEST: u8 = 15;
// const ICMP_TYPE_INFO_REPLY: u8 = 16;

// // UNREACH
// const ICMP_CODE_NET_UNREACH: u8 = 0;
// const ICMP_CODE_HOST_UNREACH: u8 = 1;
// const ICMP_CODE_PROTO_UNREACH: u8 = 2;
// const ICMP_CODE_PORT_UNREACH: u8 = 3;
// const ICMP_CODE_FRAGMENT_NEEDED: u8 = 4;
// const ICMP_CODE_SOURCE_ROUTE_FAILED: u8 = 5;

// // REDIRECT
// const ICMP_CODE_REDIRECT_NET: u8 = 0;
// const ICMP_CODE_REDIRECT_HOST: u8 = 1;
// const ICMP_CODE_REDIRECT_TOS_NET: u8 = 2;
// const ICMP_CODE_REDIRECT_TOS_HOST: u8 = 3;

// // TIME_EXEEDED
// const ICMP_CODE_EXCEEDED_TTL: u8 = 0;
// const ICMP_CODE_EXCEEDED_FRAGMENT: u8 = 1;

#[repr(packed)]
pub struct ICMPHeader {
    icmp_type: u8,
    code: u8,
    check_sum: u16,
    values: u32,
}

// pub struct ICMPEcho {
//     icmp_type: u8,
//     code: u8,
//     check_sum: u16,
//     id: u16,
//     seq: u16,
// }

pub fn input(
    data: &[u8],
    len: usize,
    src: IPAdress,
    mut dst: IPAdress,
    device: &mut NetDevice,
    iface: &IPInterface,
    contexts: &mut ProtocolContexts,
    pcbs: &mut ControlBlocks,
) -> Result<(), ()> {
    let icmp_hdr_size = size_of::<ICMPHeader>();
    let hdr = unsafe { bytes_to_struct::<ICMPHeader>(data) };

    println!("ICMP input: type = {:x?}", hdr.icmp_type);

    let sum = cksum16(data, len, 0);
    if sum != 0 {
        println!("Checksum failed: {sum}");
        return Err(());
    }

    if hdr.icmp_type == ICMP_TYPE_ECHO {
        let icmp_data = data[icmp_hdr_size..].to_vec();
        if dst != iface.unicast {
            // change original destination when addressed to broadcast address
            dst = iface.unicast;
        }
        output(
            ICMP_TYPE_ECHOREPLY,
            hdr.code,
            hdr.values,
            icmp_data,
            len - icmp_hdr_size,
            dst, // src becomes dst for replying
            src, // dst becomes src for replying
            device,
            contexts,
            pcbs,
        );
    }
    Ok(())
}

pub fn output(
    icmp_type: u8,
    code: u8,
    values: u32,
    mut icmp_data: Vec<u8>,
    len: usize,
    src: IPAdress,
    dst: IPAdress,
    device: &mut NetDevice,
    contexts: &mut ProtocolContexts,
    pcbs: &mut ControlBlocks,
) {
    let hlen = size_of::<ICMPHeader>();
    let hdr = ICMPHeader {
        icmp_type,
        code,
        check_sum: 0,
        values,
    };
    // Add data after header
    let header_bytes = unsafe { to_u8_slice::<ICMPHeader>(&hdr) };
    let mut data = header_bytes.to_vec();
    data.append(&mut icmp_data);

    let check_sum = cksum16(&data, hlen + len, 0);
    // Update checksum in byte data
    data[2] = ((check_sum & 0xff00) >> 8) as u8;
    data[3] = (check_sum & 0xff) as u8;

    super::output(IPProtocolType::Icmp, data, src, dst, device, contexts).unwrap();
}
