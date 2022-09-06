pub mod tap;

#[derive(Debug)]
pub enum DriverType {
    Tap,
    Pcap,
}

#[derive(Debug)]
pub struct DriverData {
    fd: i32,
    irq: i32,
}

impl DriverData {
    pub fn new(fd: i32, irq: i32) -> DriverData {
        DriverData { fd, irq }
    }
}
