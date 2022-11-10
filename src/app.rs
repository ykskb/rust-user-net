use crate::devices::ethernet;
use crate::devices::loopback;
use crate::devices::{NetDeviceType, NetDevices};
use crate::protocols::arp::ArpTable;
use crate::protocols::ip::icmp;
use crate::protocols::ip::ip_addr_to_bytes;
use crate::protocols::ip::ip_addr_to_str;
use crate::protocols::ip::tcp;
use crate::protocols::ip::udp;
use crate::protocols::ip::{
    IPAdress, IPEndpoint, IPHeaderIdManager, IPInterface, IPRoute, IPRoutes,
};
use crate::protocols::{ControlBlocks, NetProtocol, NetProtocols, ProtocolContexts, ProtocolType};
use crate::util::le_to_be_u32;
use std::process;
use std::sync::Mutex;
use std::{
    sync::{
        mpsc::{self, TryRecvError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const LOOPBACK_IP: &str = "127.0.0.1";
const LOOPBACK_NETMASK: &str = "255.255.255.0";
const DEFAULT_GATEWAY: &str = "192.0.2.1";
const ETH_TAP_IP: &str = "192.0.2.2";
const ETH_TAP_NETMASK: &str = "255.255.255.0";

pub struct NetApp {
    pub devices: Arc<Mutex<NetDevices>>,
    pub protocols: Arc<Mutex<NetProtocols>>,
    pub contexts: Arc<Mutex<ProtocolContexts>>,
    pub pcbs: Arc<Mutex<ControlBlocks>>,
}

impl NetApp {
    pub fn new() -> NetApp {
        let mut devices = NetDevices::new();
        let mut ip_routes = IPRoutes::new();
        // Loopback device
        let mut loopback_device = loopback::init(0);
        loopback_device.open().unwrap();

        // Loopback interface
        let loopback_interface = Arc::new(IPInterface::new(LOOPBACK_IP, LOOPBACK_NETMASK));
        loopback_device.register_interface(loopback_interface.clone());

        // Loopback route
        let loopback_route = IPRoute::interface_route(loopback_interface);

        devices.register(loopback_device);
        ip_routes.register(loopback_route);

        // Ethernet device
        let mut ethernet_device = ethernet::init(1, crate::drivers::DriverType::Tap);
        ethernet_device.open().unwrap();

        // Ethernet Interface
        let ethernet_interface = Arc::new(IPInterface::new(ETH_TAP_IP, ETH_TAP_NETMASK));
        ethernet_device.register_interface(ethernet_interface.clone());

        devices.register(ethernet_device);

        // Default gateway route
        let default_gw_route = IPRoute::gateway_route(DEFAULT_GATEWAY, ethernet_interface);
        ip_routes.register(default_gw_route);

        // Protocol setup
        let mut protocols = NetProtocols::new();

        // ARP
        let arp_proto = NetProtocol::new(ProtocolType::Arp);
        protocols.register(arp_proto);

        // IP
        let ip_proto = NetProtocol::new(ProtocolType::IP);
        protocols.register(ip_proto);

        // Protocol contexts
        let contexts = ProtocolContexts {
            arp_table: ArpTable::new(),
            ip_routes,
            ip_id_manager: IPHeaderIdManager::new(),
        };

        NetApp {
            devices: Arc::new(Mutex::new(devices)),
            protocols: Arc::new(Mutex::new(protocols)),
            contexts: Arc::new(Mutex::new(contexts)),
            pcbs: Arc::new(Mutex::new(ControlBlocks::new())),
        }
    }

    pub fn run(&self, receiver: mpsc::Receiver<()>) -> JoinHandle<()> {
        let devices_arc = self.devices.clone();
        let protocols_arc = self.protocols.clone();
        let contexts_arc = self.contexts.clone();
        let pcbs_arc = self.pcbs.clone();
        let mut soc_opt = None;

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

            let do_test = true;
            if !do_test {
                continue;
            }

            let data: Vec<u8> = vec![
                0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x30, 0x21, 0x40, 0x23, 0x24,
                0x25, 0x5e, 0x26, 0x2a, 0x28, 0x29, 0x41, 0x42, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67,
                0x68, 0x69,
            ];
            let data_len = data.len();

            if soc_opt.is_none() {
                // TEST: TCP connection
                let local = IPEndpoint::new_from_str("0.0.0.0", 7);
                soc_opt = Some(
                    tcp::rfc793_open(
                        local,
                        None,
                        false,
                        pcbs_arc.clone(),
                        devices_arc.clone(),
                        contexts_arc.clone(),
                    )
                    .unwrap(),
                );

                // TEST: UDP receive
                // soc_opt = {
                //     let pcbs = &mut pcbs_arc.lock().unwrap();
                //     let soc = udp::open(&mut pcbs.udp_pcbs);
                //     let local = IPEndpoint::new_from_str("0.0.0.0", 7);
                //     udp::bind(&mut pcbs.udp_pcbs, soc, local);
                //     Some(soc)
                // };
            }
            {
                let pcbs = &mut pcbs_arc.lock().unwrap();
                let devices = &mut devices_arc.lock().unwrap();
                let contexts = &mut contexts_arc.lock().unwrap();
                let eth_device = devices.get_mut_by_type(NetDeviceType::Ethernet).unwrap();
                let soc = soc_opt.unwrap();

                // TEST: UDP send
                // let remote = IPEndpoint::new_from_str("192.0.2.1", 10007);
                // udp::send_to(soc, data, remote, eth_device, contexts, pcbs);

                // TEST: UDP receive & send
                // let received = udp::receive_from(soc, pcbs_arc.clone());

                // if received.is_some() {
                //     let data_entry = received.unwrap();
                //     println!(
                //         "Sock num: {soc} Received: {:?} from {:?}",
                //         data_entry.data,
                //         ip_addr_to_str(data_entry.remote_endpoint.address)
                //     );
                //     {
                //         udp::send_to(
                //             soc,
                //             data_entry.data,
                //             data_entry.remote_endpoint,
                //             eth_device,
                //             contexts,
                //             pcbs,
                //         );
                //     }
                // }

                // TEST: ICMP output
                // let pid = process::id() % u16::MAX as u32;
                // let values = le_to_be_u32(pid << 16 | 1);
                // let icmp_type_echo: u8 = 8;
                // let ip_any = 0;
                // let dst = ip_addr_to_bytes("8.8.8.8").unwrap();
                // icmp::output(
                //     icmp_type_echo,
                //     0,
                //     values,
                //     data,
                //     data_len,
                //     ip_any,
                //     dst,
                //     eth_device,
                //     contexts,
                //     pcbs,
                // );
            }

            // TEST: TCP
        })
    }

    pub fn close_sockets(&mut self) {
        let mut pcbs = self.pcbs.lock().unwrap();
        pcbs.udp_pcbs.close_sockets();
        pcbs.tcp_pcbs.close_sockets();
    }

    pub fn handle_protocol(&mut self) {
        let devices = &mut self.devices.lock().unwrap();
        let protocols = &mut self.protocols.lock().unwrap();
        let contexts = &mut self.contexts.lock().unwrap();
        let pcbs = &mut self.pcbs.lock().unwrap();
        protocols.handle_data(devices, contexts, pcbs);
    }

    pub fn handle_irq(&mut self, irq: i32) {
        let devices = &mut self.devices.lock().unwrap();
        let protocols = &mut self.protocols.lock().unwrap();
        devices.handle_irq(irq, protocols);
    }

    pub fn tcp_transmit_thread(&mut self, receiver: mpsc::Receiver<()>) -> JoinHandle<()> {
        let pcbs_arc = self.pcbs.clone();
        let devices_arc = self.devices.clone();
        let contexts_arc = self.contexts.clone();
        thread::spawn(move || loop {
            // transmit check interval: 100ms
            thread::sleep(Duration::from_millis(100));

            // Termination check
            match receiver.try_recv() {
                Ok(_) | Err(TryRecvError::Disconnected) => {
                    println!("TCP transmit thread Terminating.");
                    break;
                }
                Err(TryRecvError::Empty) => {}
            }

            {
                let pcbs = &mut pcbs_arc.lock().unwrap();
                let devices = &mut devices_arc.lock().unwrap();
                let contexts = &mut contexts_arc.lock().unwrap();
                let eth_device = devices.get_mut_by_type(NetDeviceType::Ethernet).unwrap();
                tcp::retransmit(&mut pcbs.tcp_pcbs, eth_device, contexts);
            }
        })
    }
}
