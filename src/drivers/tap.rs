use super::DriverData;
use std::io;
use crate::{
    devices::{
        ethernet::{ETH_ADDR_ANY, ETH_FRAME_MAX},
        NetDevice, NET_DEVICE_ADDR_LEN,
    },
    interrupt::INTR_IRQ_BASE
};
use core::slice;
use ifstructs::ifreq;
use ioctl::*;
use nix::{
    errno::errno,
    libc::{c_int, fcntl, F_SETFL, F_SETOWN, IFF_NO_PI, IFF_TAP, O_ASYNC, SIOCGIFHWADDR},
    poll::{self, PollFd, PollFlags},
    sys::socket::{socket, AddressFamily, SockFlag, SockType},
    unistd::{read, write}, 
};
use std::{fs::OpenOptions, os::unix::prelude::AsRawFd, process};

const TUN_PATH: &str = "/dev/net/tun";
const TUN_IOC_MAGIC: u8 = b'T';
const TUN_IOC_SET_IFF: u8 = 202;

const F_SETSIG: c_int = 10; // not defined in nix
const AF_INET_RAW: u16 = 2;

// const SOCK_IOC_TYPE: u8 = 0x89; // uapi/linux/sockios.h

const ETH_TAP_IRQ: i32 = INTR_IRQ_BASE + 2;

const EINTR: i32 = 4;

// Sets interface flag with IFR
ioctl!(write tun_set_iff with TUN_IOC_MAGIC, TUN_IOC_SET_IFF; c_int);

// Gets hardware address of a socket
ioctl!(bad read get_hw_addr with SIOCGIFHWADDR; ifreq);


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
        if get_hw_addr(soc, &mut ifr) < 0 {
            let err = io::Error::last_os_error();
            println!("Get IF HW Addr failed: {err}");
            panic!();
        }

        let hw_addr_u8 = slice::from_raw_parts(
            ifr.ifr_ifru.ifr_hwaddr.sa_data.as_ptr() as *const u8,
            NET_DEVICE_ADDR_LEN,
        );
        let name = ifr.get_name().unwrap();
        println!("Retrieved HW Address for {name}: {:x?}", hw_addr_u8);
        device.address.copy_from_slice(hw_addr_u8);
    }
}

pub fn open(device: &mut NetDevice) {
    let file = OpenOptions::new().read(true).write(true).open(TUN_PATH).unwrap();
    let fd = file.as_raw_fd();

    let mut ifr = ifreq::from_name(&device.name).unwrap();
    let ifr_flag = IFF_TAP | IFF_NO_PI;
    ifr.set_flags(ifr_flag as i16);

    unsafe {
        if tun_set_iff(fd, &mut ifr as *mut _ as *mut _) < 0 {
            let err = io::Error::last_os_error();
            println!("TUN set IFF failed: {err}");
            panic!();
        }

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

pub fn read_data(device: &NetDevice) -> (usize, [u8; ETH_FRAME_MAX]) {
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
        return (len, buf);
    }
    (0, buf)
}

pub fn write_data(device: &NetDevice, data: &[u8]) -> Result<(), ()> {
    let fd = device.driver_data.as_ref().unwrap().fd;
    write(fd, data).unwrap();
    Ok(())
}
