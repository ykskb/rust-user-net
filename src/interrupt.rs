use std::sync::Arc;

pub const INTR_IRQ_BASE: i32 = 35; // SIGRTMIN: 34 & SIGRTMAX: 64

#[derive(Debug)]
pub struct IRQEntry {
    pub irq: i32,
    flags: u8,
    next: Option<Box<IRQEntry>>,
    pub custom_data: Option<Arc<[u8]>>,
}

impl<'a> IRQEntry {
    pub fn new(irq: i32, flags: u8) -> IRQEntry {
        IRQEntry {
            irq,
            flags,
            next: None,
            custom_data: None,
        }
    }
}
