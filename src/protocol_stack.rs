use crate::devices::loopback;
use crate::devices::NetDevice;
use crate::net::IPInterface;
use crate::protocols::NetProtocol;
use crate::protocols::ProtocolType;
use crate::util::List;
use std::sync::mpsc::TryRecvError;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct ProtoStackSetup {
    devices: Arc<Mutex<List<NetDevice>>>,
    protocols: Arc<Mutex<List<NetProtocol>>>,
}

impl ProtoStackSetup {
    pub fn new() -> ProtoStackSetup {
        let mut lo_device = loopback::init(0);
        let ip_interface = IPInterface::new("127.0.0.1", "255.255.255.0");
        lo_device.open().unwrap();
        lo_device.register_interface(ip_interface);
        let mut devices = List::<NetDevice>::new();
        devices.push(lo_device);

        let ip_proto = NetProtocol::new(ProtocolType::IP);
        let mut protocols = List::<NetProtocol>::new();
        protocols.push(ip_proto);

        ProtoStackSetup {
            devices: Arc::new(Mutex::new(devices)),
            protocols: Arc::new(Mutex::new(protocols)),
        }
    }

    pub fn run(&self, receiver: mpsc::Receiver<()>) -> JoinHandle<()> {
        let device = Arc::clone(&self.devices);
        thread::spawn(move || loop {
            // Termination check
            match receiver.try_recv() {
                Ok(_) | Err(TryRecvError::Disconnected) => {
                    println!("App thread Terminating.");
                    break;
                }
                Err(TryRecvError::Empty) => {}
            }

            let mut device_mutex = device.lock().unwrap();
            let device = device_mutex.iter_mut().next().unwrap();
            let data = Arc::new(vec![3, 4, 5, 6]);
            device.transmit(ProtocolType::IP, data, 4, [0; 6]).unwrap();
            drop(device_mutex);

            thread::sleep(Duration::from_millis(2000));
        })
    }

    /// Calls ISR handler of a device with a matching IRQ, passing protocols.
    pub fn handle_irq(&self, irq: i32) {
        let device_mutex = self.devices.lock().unwrap();

        for device in device_mutex.iter() {
            if device.irq_entry.irq == irq {
                let mut protocol_mutex = self.protocols.lock().unwrap();
                device.isr(irq, &mut protocol_mutex);
            }
        }
    }

    /// Triggers data queue check for all protocols.
    pub fn handle_protocol(&self) {
        let devices = self.devices.lock().unwrap();
        let mut protocols = self.protocols.lock().unwrap();

        for protocol in protocols.iter_mut() {
            protocol.handle_input(&devices);
        }
    }
}
