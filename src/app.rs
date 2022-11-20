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
use clap::{Args, Parser, Subcommand};
use log::{info, warn};
use std::process;
use std::str;
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
        // Args
        let args = Cli::parse();

        // Setups
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

    pub fn run(&mut self, receiver: mpsc::Receiver<()>) -> JoinHandle<()> {
        let args = Cli::parse();
        match args.command {
            Commands::Tcp(tcp) => {
                let tcp_command = tcp.command.unwrap();
                match tcp_command {
                    EndPointCommand::Send {
                        target_ip,
                        target_port,
                        data,
                    } => {
                        return self.tcp_send_command(target_ip, target_port, data, receiver);
                    }
                    EndPointCommand::Receive {
                        local_ip,
                        local_port,
                    } => {
                        return self.tcp_receive_command(receiver);
                    }
                };
            }
            Commands::Udp(udp) => {
                let udp_command = udp.command.unwrap();
                match udp_command {
                    EndPointCommand::Send {
                        target_ip,
                        target_port,
                        data,
                    } => {
                        return self.udp_send_command(target_ip, target_port, data, receiver);
                    }
                    EndPointCommand::Receive {
                        local_ip,
                        local_port,
                    } => {
                        return self.udp_receive_command(receiver);
                    }
                }
            }
        }
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
                    info!("TCP transmit thread Terminating.");
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

    // CLI command implementations

    fn tcp_send_command(
        &mut self,
        target_ip: String,
        target_port: u16,
        data: String,
        receiver: mpsc::Receiver<()>,
    ) -> JoinHandle<()> {
        let pcbs_arc = self.pcbs.clone();
        let devices_arc = self.devices.clone();
        let contexts_arc = self.contexts.clone();
        let mut sock_opt = None;
        let mut request_sent = false;
        thread::spawn(move || loop {
            // Termination check
            match receiver.try_recv() {
                Ok(_) | Err(TryRecvError::Disconnected) => {
                    info!("App: thread terminating.");
                    break;
                }
                Err(TryRecvError::Empty) => {}
            }
            if sock_opt.is_none() {
                sock_opt = {
                    let local = IPEndpoint::new_from_str("192.0.2.2", 7);
                    let remote = IPEndpoint::new_from_str(&target_ip, target_port);
                    tcp::rfc793_open(
                        local,
                        Some(remote),
                        true,
                        pcbs_arc.clone(),
                        devices_arc.clone(),
                        contexts_arc.clone(),
                    )
                }
            }
            if !request_sent {
                info!("CLI: sending request");
                let devices = &mut devices_arc.lock().unwrap();
                let contexts = &mut contexts_arc.lock().unwrap();
                let eth_device = devices.get_mut_by_type(NetDeviceType::Ethernet).unwrap();

                let req = data
                    .replace("\\r", "\r")
                    .replace("\\n", "\n")
                    .as_bytes()
                    .to_vec(); //  "GET / HTTP/1.1\r\nHost: www.google.com\r\n\r\n"
                tcp::send(
                    sock_opt.unwrap(),
                    req,
                    eth_device,
                    contexts,
                    &mut pcbs_arc.clone(),
                );
                request_sent = true;
            }
            info!("CLI: starting TCP receive...");
            let receive_res = tcp::receive(sock_opt.unwrap(), 2048, pcbs_arc.clone());
            if let Some(received) = receive_res {
                log_data(&received[..]);
            }
        })
    }

    fn tcp_receive_command(&mut self, receiver: mpsc::Receiver<()>) -> JoinHandle<()> {
        let pcbs_arc = self.pcbs.clone();
        let devices_arc = self.devices.clone();
        let contexts_arc = self.contexts.clone();
        let mut sock_opt = None;
        thread::spawn(move || loop {
            // Termination check
            match receiver.try_recv() {
                Ok(_) | Err(TryRecvError::Disconnected) => {
                    info!("App: thread terminating.");
                    break;
                }
                Err(TryRecvError::Empty) => {}
            }
            if sock_opt.is_none() {
                sock_opt = {
                    let local = IPEndpoint::new_from_str("0.0.0.0", 7);
                    tcp::rfc793_open(
                        local,
                        None,
                        false,
                        pcbs_arc.clone(),
                        devices_arc.clone(),
                        contexts_arc.clone(),
                    )
                }
            }
            if sock_opt.is_none() {
                info!("CLI: interrupted before establishing any connection.");
                return;
            }
            info!("CLI: starting TCP receive...");
            let receive_res = tcp::receive(sock_opt.unwrap(), 2048, pcbs_arc.clone());
            if let Some(received) = receive_res {
                log_data(&received[..]);
            }
        })
    }

    fn udp_send_command(
        &mut self,
        target_ip: String,
        target_port: u16,
        data: String,
        receiver: mpsc::Receiver<()>,
    ) -> JoinHandle<()> {
        let pcbs_arc = self.pcbs.clone();
        let devices_arc = self.devices.clone();
        let contexts_arc = self.contexts.clone();
        let mut soc_opt = None;
        let mut sent_count = 0;

        thread::spawn(move || loop {
            // Termination check
            match receiver.try_recv() {
                Ok(_) | Err(TryRecvError::Disconnected) => {
                    info!("App: thread terminating.");
                    break;
                }
                Err(TryRecvError::Empty) => {}
            }
            if soc_opt.is_none() {
                soc_opt = {
                    let pcbs = &mut pcbs_arc.lock().unwrap();
                    let soc = udp::open(&mut pcbs.udp_pcbs);
                    let local = IPEndpoint::new_from_str("0.0.0.0", 7);
                    udp::bind(&mut pcbs.udp_pcbs, soc, local);
                    Some(soc)
                }
            }
            // send twice to wait for ARP response once
            if sent_count < 2 {
                let devices = &mut devices_arc.lock().unwrap();
                let contexts = &mut contexts_arc.lock().unwrap();
                let pcbs = &mut pcbs_arc.lock().unwrap();

                let remote = IPEndpoint::new_from_str(&target_ip, target_port); // 192.0.2.1 10007
                let eth_device = devices.get_mut_by_type(NetDeviceType::Ethernet).unwrap();
                let req = data
                    .replace("\\r", "\r")
                    .replace("\\n", "\n")
                    .as_bytes()
                    .to_vec();

                udp::send_to(soc_opt.unwrap(), req, remote, eth_device, contexts, pcbs);
                sent_count += 1;
            } else {
                info!("CLI: starting UDP receive...");
                let receive_res = udp::receive_from(soc_opt.unwrap(), pcbs_arc.clone());
                if let Some(entry) = receive_res {
                    log_data(&entry.data[..]);
                }
            }
            // TODO: fix this hack to wait for ARP reply in signal thread
            thread::sleep(Duration::from_secs(1));
        })
    }

    fn udp_receive_command(&self, receiver: mpsc::Receiver<()>) -> JoinHandle<()> {
        let pcbs_arc = self.pcbs.clone();
        let mut soc_opt = None;
        thread::spawn(move || loop {
            // Termination check
            match receiver.try_recv() {
                Ok(_) | Err(TryRecvError::Disconnected) => {
                    info!("App: thread terminating.");
                    break;
                }
                Err(TryRecvError::Empty) => {}
            }
            if soc_opt.is_none() {
                soc_opt = {
                    let pcbs = &mut pcbs_arc.lock().unwrap();
                    let soc = udp::open(&mut pcbs.udp_pcbs);
                    let local = IPEndpoint::new_from_str("0.0.0.0", 7);
                    udp::bind(&mut pcbs.udp_pcbs, soc, local);
                    Some(soc)
                }
            }
            info!("CLI: starting UDP receive...");
            let receive_res = udp::receive_from(soc_opt.unwrap(), pcbs_arc.clone());
            if let Some(entry) = receive_res {
                log_data(&entry.data[..]);
            }
        })
    }
}

fn log_data(data: &[u8]) {
    let received_utf8 = str::from_utf8(data);
    if let Ok(utf8_str) = received_utf8 {
        info!("CLI: data received = {:?}", utf8_str);
    } else {
        warn!("CLI: UTF8 error. Data is {:02x?}", data);
    }
}
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

// CLI setup

#[derive(Debug, Parser)]
#[command(name = "rust-user-net")]
#[command(about = "Network protocol stack in user space written in Rust.", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Tcp(Tcp),
    Udp(Udp),
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
#[command(about = "Sends and/or receive TCP packets. `rust-user-net tcp -h` for more details.", long_about = None)]
struct Tcp {
    #[command(subcommand)]
    command: Option<EndPointCommand>,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
#[command(about = "Sends and/or receive UDP packets. `rust-user-net udp -h` for more details.", long_about = None)]
struct Udp {
    #[command(subcommand)]
    command: Option<EndPointCommand>,
}

#[derive(Debug, Subcommand)]
enum EndPointCommand {
    #[command(about = "Sends a request with data and starts a receive loop printing each segment received. Ctrl+C to end.", long_about = None)]
    Send {
        target_ip: String,
        target_port: u16,
        data: String,
    },
    #[command(about = "Starts a receive loop printing out each segment received. Ctrl+C to end.", long_about = None)]
    Receive {
        local_ip: String,
        local_port: String,
    },
}
