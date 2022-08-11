use crate::device;

pub const INTR_IRQ_BASE: i32 = 35; // SIGRTMIN: 34 & SIGRTMAX: 64
pub const INTR_IRQ_NULL: i32 = INTR_IRQ_BASE;

pub struct IRQEntry {
    irq: u8,
    flags: u8,
    name: String,
    handler: fn(),
    device: Box<device::NetDevice>,
    next: Option<Box<IRQEntry>>,
}

impl IRQEntry {
    pub fn new(
        irq: u8,
        flags: u8,
        name: String,
        handler: fn(),
        dvc: Box<device::NetDevice>,
    ) -> IRQEntry {
        IRQEntry {
            irq,
            flags,
            name,
            handler,
            device: dvc,
            next: None,
        }
    }
}
