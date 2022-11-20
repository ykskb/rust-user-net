pub mod pcap;
pub mod tap;

use std::fs::File;

#[derive(Debug)]
pub enum DriverType {
    Tap,
    Pcap,
}

#[derive(Debug)]
pub struct DriverData {
    // pub fd: i32,
    pub file: File,
    irq: i32,
}

impl DriverData {
    pub fn new(file: File, irq: i32) -> DriverData {
        DriverData { file, irq }
    }
}
