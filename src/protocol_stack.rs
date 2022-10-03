use crate::devices::ethernet;
use crate::devices::loopback;
use crate::devices::NetDevice;
use crate::protocols::arp::ArpTable;
use crate::protocols::ip::IPInterface;
use crate::protocols::ip::IPRoute;
use crate::protocols::NetProtocol;
use crate::protocols::ProtocolType;
use crate::util::List;
use std::sync::mpsc::TryRecvError;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const LOOPBACK_IP: &str = "127.0.0.1";
const LOOPBACK_NETMASK: &str = "255.255.255.0";
const DEFAULT_GATEWAY: &str = "192.0.2.1";

pub struct ProtoStackSetup {
    devices: Arc<Mutex<List<NetDevice>>>,
    protocols: Arc<Mutex<List<NetProtocol>>>,
    arp_table: Arc<Mutex<ArpTable>>,
    ip_routes: Arc<Mutex<List<IPRoute>>>,
}

impl ProtoStackSetup {
    pub fn new() -> ProtoStackSetup {
        let mut devices = List::<NetDevice>::new();
        let mut ip_routes = List::<IPRoute>::new();

        // Loopback device
        let mut loopback_device = loopback::init(0);
        loopback_device.open().unwrap();

        // Loopback interface
        let loopback_interface = Arc::new(IPInterface::new(LOOPBACK_IP, LOOPBACK_NETMASK));
        loopback_device.register_interface(loopback_interface.clone());

        devices.push(loopback_device);

        // Ethernet device
        let mut ethernet_device = ethernet::init(1, crate::drivers::DriverType::Tap);
        ethernet_device.open().unwrap();

        devices.push(ethernet_device);

        // Loopback route
        let loopback_route = IPRoute::interface_route(loopback_interface);
        ip_routes.push(loopback_route);

        // Default gateway
        // let default_gateway_route = IPRoute::gateway_route(gateway_ip, interface);

        ProtoStackSetup {
            devices: Arc::new(Mutex::new(devices)),
            protocols: Arc::new(Mutex::new(Self::setup_protocols())),
            arp_table: Arc::new(Mutex::new(ArpTable::new())),
            ip_routes: Arc::new(Mutex::new(ip_routes)),
        }
    }

    fn setup_protocols() -> List<NetProtocol> {
        let mut protocols = List::<NetProtocol>::new();
        // ARP
        let arp_proto = NetProtocol::new(ProtocolType::Arp);
        protocols.push(arp_proto);
        // IP
        let ip_proto = NetProtocol::new(ProtocolType::IP);
        protocols.push(ip_proto);
        protocols
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
            let data = vec![3, 4, 5, 6];
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
        let mut devices = self.devices.lock().unwrap();
        let mut protocols = self.protocols.lock().unwrap();
        let mut arp_table = self.arp_table.lock().unwrap();
        let ip_routes = self.ip_routes.lock().unwrap();

        for protocol in protocols.iter_mut() {
            protocol.handle_input(&mut devices, &mut arp_table, &ip_routes);
        }
    }
}
