use std::sync::mpsc::TryRecvError;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::device::{self, NetDevice};

pub struct ProtoStackSetup {
    devices: Arc<Mutex<Option<NetDevice>>>,
}

impl ProtoStackSetup {
    pub fn new() -> ProtoStackSetup {
        let mut lo_device = device::init_loopback();
        lo_device.open().unwrap();
        ProtoStackSetup {
            devices: Arc::new(Mutex::new(Some(lo_device))),
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

            let device_mutex = device.lock().unwrap();
            let device = device_mutex.as_ref().unwrap();
            let data = "TEST DATA";
            send_data(data, device).unwrap();

            thread::sleep(Duration::from_millis(1000));
        })
    }
}

pub fn send_data(data: &str, device: &NetDevice) -> Result<(), ()> {
    let data_len = data.len();
    device.transmit(0x0800, data, data_len)
}
