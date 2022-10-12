use crate::devices::ethernet;
use crate::devices::loopback;
use crate::devices::NetDevice;
use crate::devices::NetDeviceType;
use crate::protocols::arp::ArpTable;
use crate::protocols::ip::icmp;
use crate::protocols::ip::ip_addr_to_bytes;
use crate::protocols::ip::IPInterface;
use crate::protocols::ip::IPRoute;
use crate::protocols::NetProtocol;
use crate::protocols::ProtocolType;
use crate::util::le_to_be_u32;
use crate::util::List;
use std::process;
use std::sync::mpsc::TryRecvError;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const LOOPBACK_IP: &str = "127.0.0.1";
const LOOPBACK_NETMASK: &str = "255.255.255.0";
const DEFAULT_GATEWAY: &str = "192.0.2.1";
// const DEFAULT_GATEWAY: &str = "192.168.1.0";
// const DEFAULT_GATEWAY: &str = "192.168.1.254";
const ETH_TAP_IP: &str = "192.0.2.2";
const ETH_TAP_NETMASK: &str = "255.255.255.0";

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

        // Ethernet Interface
        let ethernet_interface = Arc::new(IPInterface::new(ETH_TAP_IP, ETH_TAP_NETMASK));
        ethernet_device.register_interface(ethernet_interface.clone());

        devices.push(ethernet_device);

        // Loopback route
        let loopback_route = IPRoute::interface_route(loopback_interface);
        ip_routes.push(loopback_route);

        // Default gateway route
        let default_gw_route = IPRoute::gateway_route(DEFAULT_GATEWAY, ethernet_interface);
        ip_routes.push(default_gw_route);

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

        // for test
        let arp_table = self.arp_table.clone();
        let routes = self.ip_routes.clone();

        thread::spawn(move || loop {
            // initial wait
            println!("loop sleeping for 2s...");
            thread::sleep(Duration::from_millis(2000));
            // Termination check
            match receiver.try_recv() {
                Ok(_) | Err(TryRecvError::Disconnected) => {
                    println!("App thread Terminating.");
                    break;
                }
                Err(TryRecvError::Empty) => {}
            }

            let mut device_mutex = device.lock().unwrap();
            let ip_routes = routes.lock().unwrap();
            // for test
            let do_test = true;
            if do_test {
                let mut arp_mutex = arp_table.lock().unwrap();
                for d in device_mutex.iter_mut() {
                    let pid = process::id() % u16::MAX as u32;
                    let values = le_to_be_u32(pid << 16 | 1);
                    let data: Vec<u8> = vec![
                        // 0x45, 0x00, 0x00, 0x30, 0x00, 0x80, 0x00, 0x00, 0xff, 0x01, 0xbd, 0x4a, 0x7f,
                        // 0x00, 0x00, 0x01, 0x7f, 0x00, 0x00, 0x01, 0x08, 0x00, 0x35, 0x64, 0x00, 0x80,
                        // 0x00, 0x01,
                        0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x30, 0x21, 0x40,
                        0x23, 0x24, 0x25, 0x5e, 0x26, 0x2a, 0x28, 0x29,
                    ];
                    let data_len = data.len();
                    // let icmp_header_size: usize = 8;
                    // let ip_header_min_size: usize = 20;
                    // let offset = ip_header_min_size + icmp_header_size;
                    let icmp_type_echo: u8 = 8;
                    let ip_any = 0;
                    let dst = ip_addr_to_bytes("8.8.8.8").unwrap();
                    if d.device_type == NetDeviceType::Ethernet {
                        icmp::output(
                            icmp_type_echo,
                            0,
                            values,
                            data,
                            data_len,
                            ip_any,
                            dst,
                            d,
                            &mut arp_mutex,
                            &ip_routes,
                        )
                    }
                }
                // let device = device_mutex.iter_mut().next().unwrap();
                // let data = vec![3, 4, 5, 6];
                // device.transmit(ProtocolType::IP, data, 4, [0; 6]).unwrap();
                drop(device_mutex);
                drop(arp_mutex);
                drop(ip_routes);
            }
        })
    }

    /// Calls ISR handler of a device with a matching IRQ, passing protocols.
    pub fn handle_irq(&self, irq: i32) {
        let mut device_mutex = self.devices.lock().unwrap();

        for device in device_mutex.iter_mut() {
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
