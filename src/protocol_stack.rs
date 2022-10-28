use crate::devices::NetDevice;
use crate::devices::NetDeviceType;
use crate::protocols::arp::ArpTable;
use crate::protocols::ip::icmp;
use crate::protocols::ip::udp::UdpPcbs;
use crate::protocols::ip::IPHeaderIdManager;
use crate::protocols::ip::{IPAdress, IPRoute};
use crate::protocols::NetProtocol;
use crate::protocols::ProtocolType;
use crate::util::List;

pub struct ProtocolContexts {
    pub arp_table: ArpTable,
    pub ip_routes: List<IPRoute>,
    pub ip_id_manager: IPHeaderIdManager,
    pub udp_pcbs: UdpPcbs,
}

pub struct ProtocolStack {
    pub devices: List<NetDevice>,
    pub protocols: List<NetProtocol>,
    pub contexts: ProtocolContexts,
}

impl ProtocolStack {
    pub fn new() -> ProtocolStack {
        let devices = List::<NetDevice>::new();
        let ip_routes = List::<IPRoute>::new();

        let contexts = ProtocolContexts {
            arp_table: ArpTable::new(),
            ip_routes,
            ip_id_manager: IPHeaderIdManager::new(),
            udp_pcbs: UdpPcbs::new(),
        };

        ProtocolStack {
            devices,
            protocols: Self::setup_protocols(),
            contexts,
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

    pub fn register_device(&mut self, device: NetDevice) {
        self.devices.push(device);
    }

    pub fn register_route(&mut self, route: IPRoute) {
        self.contexts.ip_routes.push(route);
    }

    /// Calls ISR handler of a device with a matching IRQ, passing protocols.
    pub fn handle_irq(&mut self, irq: i32) {
        for device in self.devices.iter_mut() {
            if device.irq_entry.irq == irq {
                device.isr(irq, &mut self.protocols);
            }
        }
    }

    /// Triggers data queue check for all protocols.
    pub fn handle_protocol(&mut self) {
        for protocol in self.protocols.iter_mut() {
            protocol.handle_input(&mut self.devices, &mut self.contexts)
        }
    }

    pub fn test_icmp(
        &mut self,
        icmp_type: u8,
        values: u32,
        data: Vec<u8>,
        src: IPAdress,
        dst: IPAdress,
    ) {
        let data_len = data.len();
        for d in self.devices.iter_mut() {
            if d.device_type == NetDeviceType::Ethernet {
                icmp::output(
                    icmp_type,
                    0,
                    values,
                    data,
                    data_len,
                    src,
                    dst,
                    d,
                    &mut self.contexts,
                );
                break;
            }
        }
    }
}
