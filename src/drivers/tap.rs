use crate::{
    devices::{
        ethernet::{EthernetHeader, ETH_ADDR_ANY, ETH_ADDR_BROADCAST, ETH_ADDR_LEN, ETH_FRAME_MAX},
        NetDevice,
    },
    interrupt::INTR_IRQ_BASE,
    protocols::{NetProtocol, ProtocolType},
    util::{bytes_to_struct, u16_to_le},
};
use ifstructs::ifreq;
use nix::{
    errno::{errno, Errno},
    ioctl_write_ptr,
    libc::{c_int, fcntl, F_SETFL, F_SETOWN, IFF_NO_PI, IFF_TAP, O_ASYNC, SIOCGIFHWADDR},
    poll::{self, PollFd, PollFlags},
    sys::socket::{socket, AddressFamily, SockFlag, SockType},
    unistd::read,
};
use std::{fs::File, mem::size_of, os::unix::prelude::AsRawFd, process};

use super::DriverData;

const TUN_PATH: &str = "/dev/net/tun";
const TUN_IOC_MAGIC: u8 = b'T';
const TUN_IOC_SET_IFF: u8 = 202;

const F_SETSIG: c_int = 10; // not defined in nix
const AF_INET_RAW: u16 = 2;

const SOCK_IOC_TYPE: u8 = 0x89; // uapi/linux/sockios.h

const ETH_TAP_IRQ: i32 = INTR_IRQ_BASE + 2;

const EINTR: i32 = 4;

// Sets interface flag with IFR
ioctl_write_ptr!(tun_set_iff, TUN_IOC_MAGIC, TUN_IOC_SET_IFF, ifreq);

// Gets hardware address of a socket
ioctl_write_ptr!(get_hw_addr, SOCK_IOC_TYPE, SIOCGIFHWADDR, ifreq);

fn set_tap_address(device: &mut NetDevice) {
    let soc = socket(
        AddressFamily::Inet,
        SockType::Datagram,
        SockFlag::empty(),
        None,
    )
    .unwrap();

    let mut ifr = ifreq::from_name(&device.name).unwrap();
    ifr.ifr_ifru.ifr_addr.sa_family = AF_INET_RAW;

    unsafe {
        get_hw_addr(soc, &ifr).unwrap();
        device.address = ifr.ifr_ifru.ifr_hwaddr.sa_data;
    }
}

pub fn open(device: &mut NetDevice) {
    let fd = File::open(TUN_PATH).unwrap().as_raw_fd();

    let mut ifr = ifreq::from_name(&device.name).unwrap();
    let ifr_flag = IFF_TAP | IFF_NO_PI;
    ifr.set_flags(ifr_flag as i16);

    unsafe {
        tun_set_iff(fd, &ifr).unwrap();

        // Signal settings for a file descriptor of TAP
        // https://man7.org/linux/man-pages/man2/fcntl.2.html

        // SIGIO & SIGURG fd signals to self process id
        let mut res = fcntl(fd, F_SETOWN, process::id());
        if res == -1 {
            panic!("F_SETOWN failed.");
        }
        // Signal enablement
        res = fcntl(fd, F_SETFL, O_ASYNC);
        if res == -1 {
            panic!("F_SETFL failed.");
        }
        // Custom signal instead of SIGIO
        res = fcntl(fd, F_SETSIG, device.irq_entry.irq);
        if res == -1 {
            panic!("F_SETSIG failed.");
        }
        if device.address[..6] == ETH_ADDR_ANY {
            set_tap_address(device);
        }
    };
    device.driver_data = Some(DriverData::new(fd, ETH_TAP_IRQ))
}

pub fn read_data(device: &NetDevice) -> Option<(ProtocolType, Vec<u8>)> {
    let fd = device.driver_data.as_ref().unwrap().fd;
    let mut poll_fds = [PollFd::new(fd, PollFlags::POLLIN)];
    let mut buf: [u8; ETH_FRAME_MAX] = [0; ETH_FRAME_MAX];
    loop {
        let ret = poll::poll(&mut poll_fds, 1000).unwrap();
        if ret == -1 {
            // signal occurred before any event
            if errno() == EINTR {
                continue;
            };
            panic!("poll failed.")
        }
        if ret == 0 {
            break;
        }
        let len = read(fd, &mut buf).unwrap();
        if len < 1 && errno() != EINTR {
            panic!("read failed.");
        }
        let hdr_len = size_of::<EthernetHeader>();
        if len < hdr_len {
            panic!("data is smaller than eth header.")
        }

        let hdr = unsafe { bytes_to_struct::<EthernetHeader>(&buf) };
        if device.address[..ETH_ADDR_LEN] != hdr.dst[..ETH_ADDR_LEN]
            || ETH_ADDR_BROADCAST != hdr.dst[..ETH_ADDR_LEN]
        {
            break;
        }
        let eth_type = u16_to_le(hdr.eth_type);
        let data = (&buf[hdr_len..]).to_vec();
        return Some((ProtocolType::from_u16(eth_type), data));
    }
    None
}
