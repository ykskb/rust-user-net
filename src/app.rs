use crate::devices::ethernet;
use crate::devices::loopback;
use crate::protocol_stack::ProtocolStack;
use crate::protocols::ip::ip_addr_to_bytes;
use crate::protocols::ip::{IPInterface, IPRoute};
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
    pub proto_stack: Arc<Mutex<ProtocolStack>>,
}

impl NetApp {
    pub fn new() -> NetApp {
        let mut proto_stack = ProtocolStack::new();

        // Loopback device
        let mut loopback_device = loopback::init(0);
        loopback_device.open().unwrap();

        // Loopback interface
        let loopback_interface = Arc::new(IPInterface::new(LOOPBACK_IP, LOOPBACK_NETMASK));
        loopback_device.register_interface(loopback_interface.clone());

        // Loopback route
        let loopback_route = IPRoute::interface_route(loopback_interface);

        proto_stack.register_device(loopback_device);
        proto_stack.register_route(loopback_route);

        // Ethernet device
        let mut ethernet_device = ethernet::init(1, crate::drivers::DriverType::Tap);
        ethernet_device.open().unwrap();

        // Ethernet Interface
        let ethernet_interface = Arc::new(IPInterface::new(ETH_TAP_IP, ETH_TAP_NETMASK));
        ethernet_device.register_interface(ethernet_interface.clone());

        proto_stack.register_device(ethernet_device);

        // Default gateway route
        let default_gw_route = IPRoute::gateway_route(DEFAULT_GATEWAY, ethernet_interface);
        proto_stack.register_route(default_gw_route);

        NetApp {
            proto_stack: Arc::new(Mutex::new(proto_stack)),
        }
    }

    pub fn run(&self, receiver: mpsc::Receiver<()>) -> JoinHandle<()> {
        let p_stack_arc = self.proto_stack.clone();

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

            let mut proto_stack = p_stack_arc.lock().unwrap();

            let pid = process::id() % u16::MAX as u32;
            let values = le_to_be_u32(pid << 16 | 1);
            let data: Vec<u8> = vec![
                // 0x45, 0x00, 0x00, 0x30, 0x00, 0x80, 0x00, 0x00, 0xff, 0x01, 0xbd, 0x4a, 0x7f,
                // 0x00, 0x00, 0x01, 0x7f, 0x00, 0x00, 0x01, 0x08, 0x00, 0x35, 0x64, 0x00, 0x80,
                // 0x00, 0x01,
                0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x30, 0x21, 0x40, 0x23, 0x24,
                0x25, 0x5e, 0x26, 0x2a, 0x28, 0x29,
            ];
            let icmp_type_echo: u8 = 8;
            let ip_any = 0;
            let dst = ip_addr_to_bytes("8.8.8.8").unwrap();

            proto_stack.test_icmp(icmp_type_echo, values, data, ip_any, dst);

            // let src_endpoint = IPEndpoint::new("0.0.0.0", 7);
            // let dst_endpoint = IPEndpoint::new("8.8.8.8", 7);
            // proto_stack.test_udp(src_endpoint, dst_endpoint, data);

            drop(proto_stack);
        })
    }

    pub fn handle_protocol(&mut self) {
        let mut proto_stack = self.proto_stack.lock().unwrap();
        proto_stack.handle_protocol();
    }

    pub fn handle_irq(&mut self, irq: i32) {
        let mut proto_stack = self.proto_stack.lock().unwrap();
        proto_stack.handle_irq(irq);
    }
}
