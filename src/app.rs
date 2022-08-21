use std::sync::mpsc::TryRecvError;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::device::{self, NetDevice};
use crate::protocol::NetProtocol;
use crate::protocol::ProtocolType;

pub struct ProtoStackSetup {
    devices: Arc<Mutex<Option<Box<NetDevice>>>>,
    protocols: Arc<Mutex<Option<Box<NetProtocol>>>>,
}

impl ProtoStackSetup {
    pub fn new() -> ProtoStackSetup {
        let mut lo_device = device::init_loopback();
        lo_device.open().unwrap();
        let ip_proto = NetProtocol::new(ProtocolType::IP);
        ProtoStackSetup {
            devices: Arc::new(Mutex::new(Some(Box::new(lo_device)))),
            protocols: Arc::new(Mutex::new(Some(Box::new(ip_proto)))),
        }
    }

    pub fn run(&self, receiver: mpsc::Receiver<()>) -> JoinHandle<()> {
        let device = Arc::clone(&self.devices);
        thread::spawn(move || loop {
            // Check termination
            match receiver.try_recv() {
                Ok(_) | Err(TryRecvError::Disconnected) => {
                    println!("App thread Terminating.");
                    break;
                }
                Err(TryRecvError::Empty) => {}
            }

            let mut device_mutex = device.lock().unwrap();
            let device = device_mutex.as_mut().unwrap();
            let data = Arc::new([3, 4, 5, 6]);
            device.transmit(ProtocolType::IP, data, 4).unwrap();
            drop(device_mutex);

            thread::sleep(Duration::from_millis(3000));
        })
    }

    /// Calls ISR handler of a device with a matching IRQ, passing protocols.
    pub fn handle_irq(&self, irq: i32) {
        let device_mutex = self.devices.lock().unwrap();
        let mut head = device_mutex.as_ref();
        while head.is_some() {
            let device = head.unwrap();
            println!("device irq: {} called irq: {}", device.irq_entry.irq, irq);
            if device.irq_entry.irq == irq {
                let mut protocol_mutex = self.protocols.lock().unwrap();
                let protocols = protocol_mutex.as_mut();
                device.isr(irq, protocols);
            }
            head = device.next_device.as_ref();
        }
    }

    /// Triggers data queue check for all protocols.
    pub fn handle_protocol(&self) {
        let mut protocol_mutex = self.protocols.lock().unwrap();
        let protocols = protocol_mutex.as_mut();

        let mut head = protocols;
        while head.is_some() {
            let protocol = head.unwrap();
            protocol.handle_input();
            head = protocol.next_protocol.as_mut();
        }
    }
}
