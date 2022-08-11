use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;

use crate::device;

pub fn run(receiver: mpsc::Receiver<()>) {
    // Device & Protocol Stack Initialization
    net_init().unwrap();
    let device = open_devices();

    thread::spawn(move || loop {
        // Check termination
        match receiver.try_recv() {
            Ok(_) | Err(TryRecvError::Disconnected) => {
                println!("App thread Terminating.");
                break;
            }
            Err(TryRecvError::Empty) => {}
        }

        let data = Box::new("TEST DATA");
        send_data(device.as_ref().unwrap().as_ref(), data).unwrap();

        thread::sleep(Duration::from_millis(1000));
    });
}

pub fn net_init() -> Result<(), ()> {
    Ok(())
}

pub fn open_devices() -> Option<Box<device::NetDevice>> {
    let mut device = Some(Box::new(device::NetDevice::new(0)));
    let mut head = &mut device;
    while head.is_some() {
        let dev = head.as_mut().unwrap();
        let d = dev.as_mut();
        d.open().unwrap();
        head = &mut d.next_device;
    }
    device
}

pub fn send_data(device: &device::NetDevice, data: Box<&str>) -> Result<(), ()> {
    let data_len = data.as_ref().len();
    device.transmit(0x0800, data, data_len)
}
