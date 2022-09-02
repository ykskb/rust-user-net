use crate::devices::NetDevice;
use ifstructs::ifreq;
use nix::{
    ioctl_write_ptr,
    libc::{c_int, fcntl, F_SETFL, F_SETOWN, IFF_NO_PI, IFF_TAP, O_ASYNC, SIOCGIFHWADDR},
    sys::socket::{socket, AddressFamily, SockFlag, SockType},
};
use std::{fs::File, os::unix::prelude::AsRawFd, process};

const TUN_PATH: &str = "/dev/net/tun";
const TUN_IOC_MAGIC: u8 = b'T';
const TUN_IOC_SET_IFF: u8 = 202;

const F_SETSIG: c_int = 10; // not defined in nix
const AF_INET_RAW: u16 = 2;

const SOCK_IOC_TYPE: u8 = 0x89; // uapi/linux/sockios.h

const ETH_ADDR_ANY: [u8; 6] = [0x00; 6];

// Sets interface flag with IFR
ioctl_write_ptr!(tun_set_iff, TUN_IOC_MAGIC, TUN_IOC_SET_IFF, ifreq);

// Gets hardware address of a socket
ioctl_write_ptr!(get_hw_addr, SOCK_IOC_TYPE, SIOCGIFHWADDR, ifreq);

pub struct TapInfo {
    name: [char; 16],
    fd: i32,
}

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

pub fn open_tap(device: &mut NetDevice) {
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
}
