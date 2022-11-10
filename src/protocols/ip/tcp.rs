use super::{ControlBlocks, ProtocolContexts};
use super::{IPAdress, IPEndpoint, IPInterface, IPProtocolType, IP_ADDR_ANY, IP_HEADER_MIN_SIZE};
use crate::devices::NetDevices;
use crate::{
    devices::NetDevice,
    protocols::ip::ip_addr_to_str,
    util::{
        be_to_le_u16, be_to_le_u32, bytes_to_struct, cksum16, le_to_be_u16, le_to_be_u32,
        to_u8_slice,
    },
};
use rand::Rng;
use std::alloc::System;
use std::{
    cmp,
    collections::VecDeque,
    mem::size_of,
    sync::{
        mpsc::{self, Sender},
        Arc, Mutex,
    },
    time::{Duration, SystemTime},
    vec,
};

const TCP_PCB_COUNT: usize = 16;
const TCP_DEFAULT_ITVL_MICROS: u64 = 200000;
const TCP_RETRANSMIT_TIMOUT_SEC: u64 = 12;
const TCP_TIMEWAIT_SEC: u64 = 30; // substitute for 2MSL
const TCP_SRC_PORT_MIN: u16 = 49152;
const TCP_SRC_PORT_MAX: u16 = 65535;

struct PseudoHeader {
    src: IPAdress,
    dst: IPAdress,
    zero: u8,
    protocol: u8,
    len: u16,
}

enum TcpFlag {
    FIN = 0x01,
    SYN = 0x02,
    RST = 0x04, // Reset
    PSH = 0x08, // Push up to receiving application immediately
    ACK = 0x10,
    URG = 0x20,
}

fn tcp_flag_is(flags: u8, flag: TcpFlag) -> bool {
    (flags & 0x3f) == flag as u8
}

fn tcp_flag_exists(flags: u8, flag: TcpFlag) -> bool {
    (flags & 0x3f) & (flag as u8) != 0
}

#[repr(packed)]
struct TcpHeader {
    src_port: u16,
    dst_port: u16,
    seq_num: u32,
    ack_num: u32,
    offset: u8, // Offset: 4 bits | Reserved: 4 out of 6 bits
    flags: u8,  // Reserved: 2 out of 6 bits | Flags: 6 bits (URG/ACK/PSH/RST/SYN/FIN)
    window: u16,
    sum: u16,
    urg_ptr: u16,
}

struct TcpSegmentInfo {
    seq_num: u32,
    ack_num: u32,
    len: u16,
    window: u16,
    urg_ptr: u16,
}

struct TcpPcbSendContext {
    next: u32,
    una: u32, // Send unacknowledged
    window: u16,
    urg_ptr: u16,
    wl1: u32, // Segment sequence number for last window update
    wl2: u32, // Segment acknowledgement number for last window update
}

struct TcpPcbRecvContext {
    next: u32,
    window: u16,
    urg_ptr: u16,
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum TcpPcbState {
    Free,
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    Closing,
    TimeWait,
    CloseWait,
    LastAck,
}

#[derive(PartialEq, Clone, Copy)]
enum TcpPcbMode {
    NotSet,
    Rfc793,
    Socket,
}

struct TcpDataQueueEntry {
    first_sent_at: SystemTime,
    last_sent_at: SystemTime,
    retry_interval: Duration,
    seq_num: u32,
    flags: u8,
    data: Vec<u8>,
}

pub struct TcpDataQueue {
    entries: VecDeque<TcpDataQueueEntry>,
}

impl TcpDataQueue {
    pub fn new() -> TcpDataQueue {
        TcpDataQueue {
            entries: VecDeque::<TcpDataQueueEntry>::new(),
        }
    }
}

pub struct TcpBacklog {
    pcb_ids: VecDeque<usize>,
}

impl TcpBacklog {
    pub fn new() -> TcpBacklog {
        TcpBacklog {
            pcb_ids: VecDeque::<usize>::new(),
        }
    }
}

pub struct TcpPcb {
    state: TcpPcbState,
    mode: TcpPcbMode,
    local: IPEndpoint,
    remote: IPEndpoint,
    send_context: TcpPcbSendContext,
    iss: u32, // Initial send sequence number
    recv_context: TcpPcbRecvContext,
    irs: u32, // Initial receive sequence number
    mtu: u16,
    mss: u16,
    buf: Vec<u8>, // [u8; 65535],
    wait_time: Option<SystemTime>,
    sender: Option<Sender<bool>>,
    data_queue: TcpDataQueue,
    parent_id: Option<usize>,
    backlog: TcpBacklog,
}

impl TcpPcb {
    pub fn new() -> TcpPcb {
        TcpPcb {
            state: TcpPcbState::Free,
            mode: TcpPcbMode::NotSet,
            local: IPEndpoint {
                address: IP_ADDR_ANY,
                port: 0,
            },
            remote: IPEndpoint {
                address: IP_ADDR_ANY,
                port: 0,
            },
            iss: 0,
            send_context: TcpPcbSendContext {
                next: 0,
                una: 0,
                window: 0,
                urg_ptr: 0,
                wl1: 0,
                wl2: 0,
            },
            recv_context: TcpPcbRecvContext {
                next: 0,
                window: 0,
                urg_ptr: 0,
            },
            irs: 0,
            mtu: 0,
            mss: 0,
            buf: Vec::new(),
            wait_time: None,
            sender: None,
            data_queue: TcpDataQueue::new(),
            parent_id: None,
            backlog: TcpBacklog::new(),
        }
    }

    pub fn add_data_entry(&mut self, seq_num: u32, flags: u8, data: Vec<u8>) {
        let now = SystemTime::now();
        let entry = TcpDataQueueEntry {
            first_sent_at: now,
            last_sent_at: now,
            retry_interval: Duration::from_micros(TCP_DEFAULT_ITVL_MICROS),
            seq_num,
            flags,
            data,
        };
        self.data_queue.entries.push_back(entry);
    }

    pub fn clean_data_queue(&mut self) {
        let mut found = false;
        let mut index_to_delete = 0;
        for (i, entry) in self.data_queue.entries.iter().enumerate() {
            if entry.seq_num >= self.send_context.una {
                break;
            }
            found = true;
            index_to_delete = i;
        }
        if found {
            self.data_queue.entries.remove(index_to_delete);
        }
    }

    pub fn release(&mut self) {
        self.state = TcpPcbState::Free;
        if self.sender.is_some() {
            self.sender.as_ref().unwrap().send(false);
        }
        self.data_queue.entries.clear();

        // TODO: close all backlog pcbs also
        // for pcb in self.backlog.pcb_ids.iter_mut() {}
        self.backlog.pcb_ids.clear();
    }

    pub fn add_backlog(&mut self, pcb_id: usize) {
        self.backlog.pcb_ids.push_back(pcb_id);
    }
}

pub struct TcpPcbs {
    pub entries: Vec<TcpPcb>,
}

impl TcpPcbs {
    pub fn new() -> TcpPcbs {
        let mut entries = Vec::with_capacity(TCP_PCB_COUNT);
        for _ in 0..TCP_PCB_COUNT {
            entries.push(TcpPcb::new());
        }
        TcpPcbs { entries }
    }

    pub fn new_entry(&mut self) -> Option<(usize, &mut TcpPcb)> {
        for (i, pcb) in self.entries.iter_mut().enumerate() {
            if pcb.state == TcpPcbState::Free {
                pcb.state = TcpPcbState::Closed;
                return Some((i, pcb));
            }
        }
        None
    }

    pub fn get_mut_by_id(&mut self, pcb_id: usize) -> Option<&mut TcpPcb> {
        self.entries.get_mut(pcb_id)
    }

    pub fn select(
        &mut self,
        local: &IPEndpoint,
        remote_opt: Option<&IPEndpoint>,
    ) -> Option<(usize, &mut TcpPcb)> {
        let mut listen_pcb = None;
        for (i, pcb) in self.entries.iter_mut().enumerate() {
            if (pcb.local.address == IP_ADDR_ANY || pcb.local.address == local.address)
                && pcb.local.port == local.port
            {
                // Bindable check for local address only
                if remote_opt.is_none() {
                    return Some((i, pcb));
                }
                let remote = remote_opt.unwrap();
                // Both remote address and port match
                if pcb.remote.address == remote.address {
                    return Some((i, pcb));
                }
                // Listen without specifying remote address
                if pcb.state == TcpPcbState::Listen {
                    if pcb.remote.address == IP_ADDR_ANY && pcb.remote.port == 0 {
                        listen_pcb = Some((i, pcb));
                    }
                }
            }
        }
        listen_pcb
    }

    pub fn close_sockets(&mut self) {
        for pcb in self.entries.iter() {
            if pcb.sender.is_some() {
                pcb.sender.as_ref().unwrap().send(false).unwrap();
            }
        }
    }
}

fn pcb_by_id(pcbs: &mut TcpPcbs, pcb_id: usize) -> &mut TcpPcb {
    pcbs.get_mut_by_id(pcb_id)
        .expect("TCP: PCB with specified id was not found.")
}

fn set_wait_time(pcb: &mut TcpPcb) {
    let addition = Duration::from_secs(TCP_TIMEWAIT_SEC);
    if pcb.wait_time.is_none() {
        pcb.wait_time = SystemTime::now().checked_add(addition);
    } else {
        pcb.wait_time.unwrap().checked_add(addition);
    }
}

pub fn retransmit(pcbs: &mut TcpPcbs, device: &mut NetDevice, contexts: &mut ProtocolContexts) {
    for pcb in pcbs.entries.iter_mut() {
        if pcb.state == TcpPcbState::Free {
            continue;
        }
        if pcb.state == TcpPcbState::TimeWait {
            if pcb.wait_time.unwrap().elapsed().unwrap().as_micros() > 0 {
                println!(
                    "TCP: timewait has elapsed for local = {:?} remote = {:?}",
                    ip_addr_to_str(pcb.local.address),
                    ip_addr_to_str(pcb.remote.address)
                );
                pcb.release();
                continue;
            }
        }
        for queue in pcb.data_queue.entries.iter_mut() {
            if queue.first_sent_at.elapsed().unwrap().as_secs() >= TCP_RETRANSMIT_TIMOUT_SEC {
                pcb.state = TcpPcbState::Closed;
                if pcb.sender.is_some() {
                    pcb.sender.as_ref().unwrap().send(false).unwrap();
                }
                continue;
            }
            let timeout = queue
                .last_sent_at
                .checked_add(queue.retry_interval)
                .unwrap();
            if timeout.elapsed().is_err() {
                // elapsed errors when time is before now
                output_segment(
                    queue.seq_num,
                    pcb.recv_context.next,
                    queue.flags,
                    pcb.recv_context.window,
                    queue.data.clone(), // TODO: fix clone
                    &pcb.local,
                    &pcb.remote,
                    device,
                    contexts,
                );
            }
        }
    }
}

pub fn output_segment(
    seq_num: u32,
    ack_num: u32,
    flags: u8,
    window: u16,
    mut tcp_data: Vec<u8>,
    local: &IPEndpoint,
    remote: &IPEndpoint,
    device: &mut NetDevice,
    contexts: &mut ProtocolContexts,
) -> usize {
    let tcp_hdr_size = size_of::<TcpHeader>();
    let tcp_data_len = tcp_data.len();
    let total_len = tcp_data_len + tcp_hdr_size;
    let tcp_header = TcpHeader {
        src_port: local.port,
        dst_port: remote.port,
        seq_num: le_to_be_u32(seq_num),
        ack_num: le_to_be_u32(ack_num),
        offset: ((tcp_hdr_size >> 2) << 4) as u8,
        flags,
        window: le_to_be_u16(window),
        sum: 0,
        urg_ptr: 0,
    };
    let pseudo_header = PseudoHeader {
        src: local.address,
        dst: remote.address,
        zero: 0,
        protocol: IPProtocolType::Tcp as u8,
        len: le_to_be_u16(total_len as u16),
    };
    let pseudo_hdr_bytes = unsafe { to_u8_slice(&pseudo_header) };
    let pseudo_sum = cksum16(pseudo_hdr_bytes, pseudo_hdr_bytes.len(), 0);

    let tcp_hdr_bytes = unsafe { to_u8_slice::<TcpHeader>(&tcp_header) };
    let mut data = tcp_hdr_bytes.to_vec();
    data.append(&mut tcp_data);
    // Update checksum
    let sum = cksum16(&data, total_len, !pseudo_sum as u32);
    data[16] = ((sum & 0xff00) >> 8) as u8;
    data[17] = (sum & 0xff) as u8;

    super::output(
        IPProtocolType::Tcp,
        data,
        local.address,
        remote.address,
        device,
        contexts,
    )
    .unwrap();
    tcp_data_len
}

pub fn output(
    pcb: &mut TcpPcb,
    flags: u8,
    data: Vec<u8>,
    device: &mut NetDevice,
    contexts: &mut ProtocolContexts,
) -> usize {
    let mut seq_num = pcb.send_context.next;
    if tcp_flag_exists(flags, TcpFlag::SYN) {
        seq_num = pcb.iss;
    }
    if (tcp_flag_exists(flags, TcpFlag::SYN) && tcp_flag_exists(flags, TcpFlag::FIN))
        || data.len() > 0
    {
        pcb.add_data_entry(seq_num, flags, data.clone()); // TODO: fix clone
    }
    output_segment(
        seq_num,
        pcb.recv_context.next,
        flags,
        pcb.recv_context.window,
        data,
        &pcb.local,
        &pcb.remote,
        device,
        contexts,
    )
}

// rfc793 section 3.9
fn segment_arrives(
    seg: TcpSegmentInfo,
    flags: u8,
    data: &[u8],
    len: usize,
    local: IPEndpoint,
    remote: IPEndpoint,
    device: &mut NetDevice,
    contexts: &mut ProtocolContexts,
    pcbs: &mut ControlBlocks,
) {
    let pcb_state;
    let pcb_id;
    let pcb_mode;

    println!("TCP: segment flag byte = {:#010b}", flags);

    {
        let pcb_opt = pcbs.tcp_pcbs.select(&local, Some(&remote));
        // No PCB or PCB is closed state
        if pcb_opt.is_none() || pcb_opt.as_ref().unwrap().1.state == TcpPcbState::Closed {
            println!("TCP: segment received for new/closed connection.");
            if tcp_flag_exists(flags, TcpFlag::RST) {
                println!("TCP: RST found. Returning...");
                return;
            }
            // Segment to unused port. Return RST.
            if tcp_flag_exists(flags, TcpFlag::ACK) {
                println!("TCP: ACK found. Replying with RST...");
                output_segment(
                    seg.ack_num,
                    0,
                    TcpFlag::RST as u8,
                    0,
                    vec![],
                    &local,
                    &remote,
                    device,
                    contexts,
                );
            } else {
                println!("TCP: non-ACK received. Replying RST-ACK...");
                output_segment(
                    0,
                    seg.seq_num + (seg.len as u32),
                    TcpFlag::RST as u8 | TcpFlag::ACK as u8,
                    0,
                    vec![],
                    &local,
                    &remote,
                    device,
                    contexts,
                );
            }
            return;
        }
        let (id, pcb) = pcb_opt.unwrap();
        pcb_state = pcb.state;
        pcb_id = id;
        pcb_mode = pcb.mode;
    }

    let mut acceptable = false;

    // Listen state
    if pcb_state == TcpPcbState::Listen {
        println!("TCP: connection in LISTEN state.");
        // Check for reset first.
        if tcp_flag_exists(flags, TcpFlag::RST) {
            return;
        }
        // Secondly check for ack.
        if tcp_flag_exists(flags, TcpFlag::ACK) {
            println!("TCP: ACK found. Replying with RST...");
            output_segment(
                seg.ack_num,
                0,
                TcpFlag::RST as u8,
                0,
                vec![],
                &local,
                &remote,
                device,
                contexts,
            );
            return;
        }
        // Third check on SYN
        if tcp_flag_exists(flags, TcpFlag::SYN) {
            println!("TCP: SYN found.");
            // Ignore: security / compartment / precedence checks
            let pcb = {
                if pcb_mode == TcpPcbMode::Socket {
                    let new_pcb = pcbs
                        .tcp_pcbs
                        .new_entry()
                        .expect("TCP: failed to allocate new pcb.")
                        .1;
                    new_pcb.mode = TcpPcbMode::Socket;
                    new_pcb.parent_id = Some(pcb_id);
                    new_pcb
                } else {
                    pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id)
                }
            };
            pcb.local = local;
            pcb.remote = remote;
            pcb.recv_context.window = pcb.buf.len() as u16;
            pcb.recv_context.next = seg.seq_num + 1;
            pcb.iss = rand::thread_rng().gen_range(0..u32::MAX);
            println!("TCP: Replying with SYN-ACK...");
            output(
                pcb,
                TcpFlag::SYN as u8 | TcpFlag::ACK as u8,
                vec![],
                device,
                contexts,
            );
            pcb.send_context.next = pcb.iss + 1;
            pcb.send_context.una = pcb.iss;
            pcb.state = TcpPcbState::SynReceived;
            // Any other incoming control or data with SYN will be processed in SYN-RECEIVED state.
            // But processing SYN or ACK should not be repeated.
            return;
        }
        // Fourth: other text or control
        return; // drop segment
    } else if pcb_state == TcpPcbState::SynSent {
        println!("TCP: connection in SYN-SENT state.");
        let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
        // First: check ACK
        if tcp_flag_exists(flags, TcpFlag::ACK) {
            if seg.ack_num <= pcb.iss || seg.ack_num > pcb.send_context.next {
                println!("TCP: ACK found with glitches. Replying with RST...");
                output_segment(
                    seg.ack_num,
                    0,
                    TcpFlag::RST as u8,
                    0,
                    vec![],
                    &local,
                    &remote,
                    device,
                    contexts,
                );
                return;
            }
            if pcb.send_context.una <= seg.ack_num && seg.ack_num <= pcb.send_context.next {
                acceptable = true;
            }
        }
        // Second: check RST
        if tcp_flag_exists(flags, TcpFlag::RST) {
            if acceptable {
                println!("TCP: RST found. Closing connection.");
                pcb.release();
            }
            return;
        }
        // Third: check security and precedence (ignored)
        // Fourth: check SYN
        if tcp_flag_exists(flags, TcpFlag::SYN) {
            println!("TCP: SYN found.");
            pcb.recv_context.next = seg.seq_num + 1;
            pcb.irs = seg.seq_num;
            if acceptable {
                pcb.send_context.una = seg.ack_num;
                pcb.clean_data_queue();
            }
            if pcb.send_context.una > pcb.iss {
                pcb.state = TcpPcbState::Established;
                println!("TCP: send.una > iss = Established. Replying with ACK...");
                output(pcb, TcpFlag::ACK as u8, vec![], device, contexts);
                // RFC793 does not specify, but send window initialization reqiured
                pcb.send_context.window = seg.window;
                pcb.send_context.wl1 = seg.seq_num;
                pcb.send_context.wl2 = seg.ack_num;
                if pcb.sender.is_some() {
                    println!("TCP: waking up sleeping PCB...");
                    pcb.sender.as_ref().unwrap().send(true).unwrap();
                }
                // Ignore: continue to sixth check on URG
            } else {
                println!("TCP: send.una <= iss = Syn-Received. Replying with SYN-ACK...");
                pcb.state = TcpPcbState::SynReceived;
                output(
                    pcb,
                    TcpFlag::SYN as u8 | TcpFlag::ACK as u8,
                    vec![],
                    device,
                    contexts,
                );
                // Ignore: other controls or texts of segment should be queued after ESTABLISHED
                return;
            }
        }
        // Fifth: neither SYN or RST so drop segment
        return;
    }

    println!(
        "TCP: connection not in LISTEN or SYN-SENT state but in {:?}",
        pcb_state
    );

    // First: check sequence number.
    if pcb_state == TcpPcbState::SynReceived
        || pcb_state == TcpPcbState::Established
        || pcb_state == TcpPcbState::FinWait1
        || pcb_state == TcpPcbState::FinWait2
        || pcb_state == TcpPcbState::CloseWait
        || pcb_state == TcpPcbState::Closing
        || pcb_state == TcpPcbState::LastAck
        || pcb_state == TcpPcbState::TimeWait
    {
        let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
        if seg.len < 1 {
            if pcb.recv_context.window < 1 {
                if seg.seq_num == pcb.recv_context.next {
                    acceptable = true;
                }
            } else {
                if pcb.recv_context.next <= seg.seq_num
                    && seg.seq_num < pcb.recv_context.next + pcb.recv_context.window as u32
                {
                    acceptable = true;
                }
            }
        } else {
            if pcb.recv_context.window < 1 {
                // not acceptable
            } else {
                if (pcb.recv_context.next <= seg.seq_num
                    && seg.seq_num < pcb.recv_context.next + pcb.recv_context.window as u32)
                    || (pcb.recv_context.next <= seg.seq_num + seg.len as u32 - 1
                        && seg.seq_num + seg.len as u32 - 1
                            < pcb.recv_context.next + pcb.recv_context.window as u32)
                {
                    acceptable = true;
                }
            }
        }
        if !acceptable {
            if tcp_flag_exists(flags, TcpFlag::RST) {
                println!("TCP: RST found and sequence/window not acceptable. Replying with ACK...");
                output(pcb, TcpFlag::ACK as u8, vec![], device, contexts);
            }
            return;
        }
        // In the following it is assumed that the segment is the idealized
        // segment that begins at RCV.NXT and does not exceed the window.
        // One could tailor actual segments to fit this assumption by
        // trimming off any portions that lie outside the window (including
        // SYN and FIN), and only processing further if the segment then
        // begins at RCV.NXT.  Segments with higher begining sequence
        // numbers may be held for later processing.
    }
    // Second: check RST bit
    if pcb_state == TcpPcbState::SynReceived {
        if tcp_flag_exists(flags, TcpFlag::RST) {
            println!("TCP: RST found for connection in SYN-RECEIVED state. Closing...");
            let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
            pcb.release();
            return;
        }
    } else if pcb_state == TcpPcbState::Established
        || pcb_state == TcpPcbState::FinWait1
        || pcb_state == TcpPcbState::FinWait2
        || pcb_state == TcpPcbState::CloseWait
    {
        if tcp_flag_exists(flags, TcpFlag::RST) {
            println!("TCP: connection reset.");
            let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
            pcb.release();
            return;
        }
    } else if pcb_state == TcpPcbState::Closing
        || pcb_state == TcpPcbState::LastAck
        || pcb_state == TcpPcbState::TimeWait
    {
        println!("TCP: connection in final state. Closing...");
        let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
        pcb.release();
        return;
    }

    // Third: security and precedence check (ignored)

    // Fourth: check SYN bit
    if pcb_state == TcpPcbState::SynReceived
        || pcb_state == TcpPcbState::Established
        || pcb_state == TcpPcbState::FinWait1
        || pcb_state == TcpPcbState::FinWait2
        || pcb_state == TcpPcbState::CloseWait
        || pcb_state == TcpPcbState::Closing
        || pcb_state == TcpPcbState::LastAck
        || pcb_state == TcpPcbState::TimeWait
    {
        if tcp_flag_exists(flags, TcpFlag::SYN) {
            println!("TCP: SYN found. Connection reset.");
            let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
            pcb.release();
            return;
        }
    }

    // Fifth: check ACK
    if !tcp_flag_exists(flags, TcpFlag::ACK) {
        return; // drop segment
    }
    println!("TCP: ACK found.");
    if pcb_state == TcpPcbState::SynReceived {
        println!("TCP: connection in SYN-RECEIVED state.");
        let mut parent_id = None;
        {
            let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
            if pcb.send_context.una <= seg.ack_num && seg.ack_num <= pcb.send_context.next {
                println!("TCP: send.una <= seg.ack = ESTABLISHED. Waking up sleeping PCB...");
                pcb.state = TcpPcbState::Established;
                if pcb.sender.is_some() {
                    pcb.sender.as_ref().unwrap().send(true).unwrap();
                }
                if pcb.parent_id.is_some() {
                    parent_id = pcb.parent_id;
                }
            } else {
                println!("TCP: send.una > seg.ack = not ESTABLISHED. Replying with RST...");
                output_segment(
                    seg.ack_num,
                    0,
                    TcpFlag::RST as u8,
                    0,
                    vec![],
                    &local,
                    &remote,
                    device,
                    contexts,
                );
                return;
            }
        }
        if parent_id.is_some() {
            println!("TCP: parent PCB found. Waking up sleeping parent PCB...");
            let parent_pcb = pcb_by_id(&mut pcbs.tcp_pcbs, parent_id.unwrap());
            parent_pcb.add_backlog(pcb_id);
            if parent_pcb.sender.is_some() {
                parent_pcb.sender.as_ref().unwrap().send(true).unwrap();
            }
        }
    } else if pcb_state == TcpPcbState::Established
        || pcb_state == TcpPcbState::FinWait1
        || pcb_state == TcpPcbState::FinWait2
        || pcb_state == TcpPcbState::CloseWait
        || pcb_state == TcpPcbState::Closing
    {
        let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
        // Received ack including unacked sequence number
        if pcb.send_context.una < seg.ack_num && seg.ack_num <= pcb.send_context.next {
            println!(
                "TCP: received ack including unacked seq number. Updating send.una with seg.ack."
            );
            pcb.send_context.una = seg.ack_num;
            pcb.clean_data_queue();

            // Ignore: users should receive positive acknowledgments for buffers which have been SENT
            // and fully acknowledged (i.e., SEND buffer should be returned with "ok" response)
            if pcb.send_context.wl1 < seg.seq_num
                || (pcb.send_context.wl1 == seg.seq_num && pcb.send_context.wl2 <= seg.ack_num)
            {
                pcb.send_context.window = seg.window;
                pcb.send_context.wl1 = seg.seq_num;
                pcb.send_context.wl2 = seg.ack_num;
            }
        } else if seg.ack_num < pcb.send_context.una {
            // Ignore: already checked ack
        } else if seg.ack_num > pcb.send_context.next {
            println!("TCP: seg.ack > send.next. Replying with ACK...");
            output(pcb, TcpFlag::ACK as u8, vec![], device, contexts);
            return;
        }
        if pcb_state == TcpPcbState::Closing {
            if seg.ack_num == pcb.send_context.next {
                println!("TCP: connection in CLOSING state and seg.ack == send.next. Waking up PCB with wait time...");
                pcb.state = TcpPcbState::TimeWait;
                set_wait_time(pcb);
                if pcb.sender.is_some() {
                    pcb.sender.as_ref().unwrap().send(true).unwrap();
                }
            }
        }
    } else if pcb_state == TcpPcbState::LastAck {
        println!("TCP: connection in LAST-ACK state.");
        let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
        if seg.ack_num == pcb.send_context.next {
            pcb.release();
        }
        return;
    } else if pcb_state == TcpPcbState::TimeWait {
        if tcp_flag_exists(flags, TcpFlag::FIN) {
            println!("TCP: FIN found for connection in TIME-WAIT state. Extending wait time...");
            let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
            set_wait_time(pcb);
        }
    }

    // Sixth: check URG (ignored)

    // Seventh: process segment text
    if pcb_state == TcpPcbState::Established
        || pcb_state == TcpPcbState::FinWait1
        || pcb_state == TcpPcbState::FinWait2
    {
        let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
        if len > 0 {
            println!("TCP: received data. Updating window, replying with ACK and waking up PCB...");
            // memcpy(pcb->buf + (sizeof(pcb->buf) - pcb->rcv.wnd), data, len);
            pcb.buf.append(&mut data.to_vec());
            pcb.recv_context.next = seg.seq_num + seg.len as u32;
            pcb.recv_context.window -= len as u16;
            output(pcb, TcpFlag::ACK as u8, vec![], device, contexts);
            if pcb.sender.is_some() {
                pcb.sender.as_ref().unwrap().send(true).unwrap();
            }
        }
    } else if pcb_state == TcpPcbState::CloseWait
        || pcb_state == TcpPcbState::Closing
        || pcb_state == TcpPcbState::LastAck
        || pcb_state == TcpPcbState::TimeWait
    {
        // Ignore: segment text
    }

    // Eighth: check FIN
    if tcp_flag_exists(flags, TcpFlag::FIN) {
        println!("TCP: FIN flag found.");
        let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
        if pcb_state == TcpPcbState::Closed
            || pcb_state == TcpPcbState::Listen
            || pcb_state == TcpPcbState::SynSent
        {
            return; // drop segment
        }

        println!("TCP: sending ACK...");
        pcb.recv_context.next = seg.seq_num + 1;
        output(pcb, TcpFlag::ACK as u8, vec![], device, contexts);

        if pcb_state == TcpPcbState::SynReceived || pcb_state == TcpPcbState::Established {
            println!("TCP: connection in SYN-RECEIVED / ESTABLISHED state. Moving to CLOSE-WAIT and waking up PCB...");
            pcb.state = TcpPcbState::CloseWait;
            if pcb.sender.is_some() {
                pcb.sender.as_ref().unwrap().send(true).unwrap();
            }
        } else if pcb_state == TcpPcbState::FinWait1 {
            if seg.ack_num == pcb.send_context.next {
                println!("TCP: connection in FIN-WAIT1 state and seg.ack == send.next. Moving to TIME-WAIT and waking up PCB...");
                pcb.state = TcpPcbState::TimeWait;
                set_wait_time(pcb);
            } else {
                println!("TCP: connection in FIN-WAIT1 state and seg.ack != send.next. Moving to CLOSING...");
                pcb.state = TcpPcbState::Closing;
            }
        } else if pcb_state == TcpPcbState::FinWait2 {
            println!("TCP: connection in FIN-WAIT2 state. Moving to TIME-WAIT...");
            pcb.state = TcpPcbState::TimeWait;
        } else if pcb_state == TcpPcbState::CloseWait {
            // Remain in CLOSE-WAIT state.
        } else if pcb_state == TcpPcbState::Closing {
            // Remain in CLOSING state.
        } else if pcb_state == TcpPcbState::LastAck {
            // Remain in LAST-ACK state.
        } else if pcb_state == TcpPcbState::TimeWait {
            // Remain in TIME-WAIT state.
            set_wait_time(pcb);
        }
    }
}

pub fn input(
    data: &[u8],
    len: usize,
    src: IPAdress,
    dst: IPAdress,
    device: &mut NetDevice,
    iface: &IPInterface,
    contexts: &mut ProtocolContexts,
    pcbs: &mut ControlBlocks,
) -> Result<(), ()> {
    let tcp_hdr_size = size_of::<TcpHeader>();
    let header = unsafe { bytes_to_struct::<TcpHeader>(data) };

    if len < tcp_hdr_size {
        panic!("TCP input: too short data.");
    }

    let pseudo_header = PseudoHeader {
        src,
        dst,
        zero: 0,
        protocol: IPProtocolType::Tcp as u8,
        len: le_to_be_u16(len as u16),
    };
    let pseudo_hdr_bytes = unsafe { to_u8_slice(&pseudo_header) };
    let pseudo_sum = !cksum16(pseudo_hdr_bytes, pseudo_hdr_bytes.len(), 0);
    let sum = cksum16(data, len, pseudo_sum as u32);
    if sum != 0 {
        println!("TCP input checksum failure: value = {sum}");
        return Err(());
    }

    if src == IP_ADDR_ANY || src == iface.broadcast || dst == IP_ADDR_ANY || dst == iface.broadcast
    {
        panic!("TCP input: only unicast is supported.");
    }

    println!(
        "TCP input: source port = {:?} destination port: {:?}",
        be_to_le_u16(header.src_port),
        be_to_le_u16(header.dst_port)
    );

    let local = IPEndpoint {
        address: dst,
        port: header.dst_port,
    };
    let remote = IPEndpoint {
        address: src,
        port: header.src_port,
    };
    let header_len = ((header.offset >> 4) << 2) as usize;
    let mut seg_len = len - header_len;
    if tcp_flag_exists(header.flags, TcpFlag::SYN) {
        seg_len += 1;
    }
    if tcp_flag_exists(header.flags, TcpFlag::FIN) {
        seg_len += 1;
    }
    let seg = TcpSegmentInfo {
        seq_num: be_to_le_u32(header.seq_num),
        ack_num: be_to_le_u32(header.ack_num),
        len: seg_len as u16,
        window: be_to_le_u16(header.window),
        urg_ptr: be_to_le_u16(header.urg_ptr),
    };

    segment_arrives(
        seg,
        header.flags,
        &data[tcp_hdr_size..],
        len - header_len,
        local,
        remote,
        device,
        contexts,
        pcbs,
    );

    Ok(())
}

// User commands (RFC793)

pub fn rfc793_open(
    local: IPEndpoint,
    remote_opt: Option<IPEndpoint>,
    active: bool,
    pcbs_arc: Arc<Mutex<ControlBlocks>>,
    devices_arc: Arc<Mutex<NetDevices>>,
    contexts_arc: Arc<Mutex<ProtocolContexts>>,
) -> Option<usize> {
    let pcb_id;
    let (sender, receiver) = mpsc::channel();
    {
        let pcbs = &mut pcbs_arc.lock().unwrap();
        let devices = &mut devices_arc.lock().unwrap();
        let contexts = &mut contexts_arc.lock().unwrap();
        let eth_device = devices
            .get_mut_by_type(crate::devices::NetDeviceType::Ethernet)
            .unwrap();

        let (new_pcb_id, pcb) = pcbs
            .tcp_pcbs
            .new_entry()
            .expect("TCP: failed to create a new PCB.");
        pcb_id = new_pcb_id;
        pcb.mode = TcpPcbMode::Rfc793;
        pcb.local = local;
        pcb.sender = Some(sender);
        if remote_opt.is_some() {
            pcb.remote = remote_opt.unwrap();
        }

        if !active {
            println!(
                "TCP: passive open with local = {:?}",
                ip_addr_to_str(pcb.local.address)
            );
            pcb.state = TcpPcbState::Listen;
        } else {
            println!(
                "TCP: active open with local = {:?} and remote = {:?}",
                ip_addr_to_str(pcb.local.address),
                ip_addr_to_str(pcb.remote.address)
            );
            pcb.recv_context.window = pcb.buf.len() as u16;
            pcb.iss = rand::thread_rng().gen_range(0..u32::MAX);

            output(pcb, TcpFlag::SYN as u8, vec![], eth_device, contexts);
            // if res.is_err() {
            //     pcb.state = TcpPcbState::Closed;
            // }
            pcb.send_context.una = pcb.iss;
            pcb.send_context.next = pcb.iss + 1;
            pcb.state = TcpPcbState::SynSent;
        }
    }
    loop {
        let wakeup = receiver.recv().unwrap();
        {
            let pcbs = &mut pcbs_arc.lock().unwrap();
            let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
            if !wakeup {
                pcb.state = TcpPcbState::Closed;
                return None;
            }
            if pcb.state == TcpPcbState::Established {
                break;
            }
            if pcb.state != TcpPcbState::SynReceived {
                pcb.state = TcpPcbState::Closed;
                return None;
            }
        }
    }

    println!("TCP rfc793_open: connection established.");
    Some(pcb_id)
}

// User commands (Socket)

pub fn open(pcbs: &mut ControlBlocks) -> usize {
    let (pcb_id, pcb) = pcbs
        .tcp_pcbs
        .new_entry()
        .expect("TCP open: failed to create a new PCB.");
    pcb.mode = TcpPcbMode::Socket;
    pcb_id
}

pub fn connect(
    pcb_id: usize,
    remote: &IPEndpoint,
    device: &mut NetDevice,
    contexts: &mut ProtocolContexts,
    pcbs_arc: &mut Arc<Mutex<ControlBlocks>>,
) -> Option<usize> {
    let mut local = {
        let pcbs = &mut pcbs_arc.lock().unwrap();
        let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
        if pcb.mode != TcpPcbMode::Socket {
            panic!("TCP: pcb is not opened as socket mode.");
        }
        IPEndpoint::new(pcb.local.address, pcb.local.port)
    };
    if local.address == IP_ADDR_ANY {
        let interface = contexts
            .ip_routes
            .get_interface(remote.address)
            .expect("TCP: interface was not found.");
        local.address = interface.unicast;
    }
    if local.port == 0 {
        let pcbs = &mut pcbs_arc.lock().unwrap();
        for port in TCP_SRC_PORT_MIN..TCP_SRC_PORT_MAX {
            local.port = port;
            if pcbs.tcp_pcbs.select(&local, Some(remote)).is_none() {
                break;
            }
        }
        if local.port == 0 {
            panic!("TCP: dynamic port assignment failed.");
        }
    }
    let (sender, receiver) = mpsc::channel();
    {
        let pcbs = &mut pcbs_arc.lock().unwrap();
        let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
        pcb.local.address = local.address;
        pcb.local.port = local.port;
        pcb.remote.address = remote.address;
        pcb.remote.port = remote.port;
        pcb.recv_context.window = pcb.buf.len() as u16;
        pcb.iss = rand::thread_rng().gen_range(0..u32::MAX);
        output(pcb, TcpFlag::SYN as u8, vec![], device, contexts);
        // close & release if fails
        pcb.send_context.una = pcb.iss;
        pcb.send_context.next = pcb.iss + 1;
        pcb.state = TcpPcbState::SynSent;
        pcb.sender = Some(sender);
    }
    loop {
        let wakeup = receiver.recv().unwrap();
        {
            let pcbs = &mut pcbs_arc.lock().unwrap();
            let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);

            if !wakeup {
                pcb.state = TcpPcbState::Closed;
                return None;
            }
            if pcb.state == TcpPcbState::Established {
                break;
            }
            if pcb.state != TcpPcbState::SynReceived {
                pcb.state = TcpPcbState::Closed;
                return None;
            }
        }
    }
    Some(pcb_id)
}

pub fn bind(pcb_id: usize, local: IPEndpoint, pcbs: &mut ControlBlocks) {
    {
        let existing = pcbs.tcp_pcbs.select(&local, None);
        if existing.is_some() {
            panic!("TCP: ip address and port already exist.");
        }
    }
    let pcb = pcbs
        .tcp_pcbs
        .get_mut_by_id(pcb_id)
        .expect("TCP: PCB with specified id was not found.");
    if pcb.mode != TcpPcbMode::Socket {
        panic!("TCP: PCB was not open in socket mode.");
    }
    pcb.local = local;
    println!(
        "TCP: bound local address = {:?} port = {:?}",
        ip_addr_to_str(pcb.local.address),
        pcb.local.port
    );
}

pub fn listen(pcb_id: usize, pcbs: &mut ControlBlocks) {
    let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
    if pcb.mode != TcpPcbMode::Socket {
        panic!("TCP: PCB was not open in socket mode.");
    }
    pcb.state = TcpPcbState::Listen;
}

pub fn accept(
    pcb_id: usize,
    remote: &IPEndpoint,
    pcbs_arc: &mut Arc<Mutex<ControlBlocks>>,
) -> Option<usize> {
    let (sender, receiver) = mpsc::channel();
    let mut next_backlog;
    {
        let pcbs = &mut pcbs_arc.lock().unwrap();
        let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
        if pcb.mode != TcpPcbMode::Socket {
            panic!("TCP: PCB was not open in socket mode.");
        }
        if pcb.state != TcpPcbState::Listen {
            panic!("TCP: PCB is not in LISTEN state.");
        }
        pcb.sender = Some(sender);
        next_backlog = pcb.backlog.pcb_ids.pop_front();
    }
    let mut backlog_id = None;
    loop {
        if next_backlog.is_some() {
            if !receiver.recv().unwrap() {
                return None;
            }
            {
                let pcbs = &mut pcbs_arc.lock().unwrap();
                let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
                if pcb.state == TcpPcbState::Closed {
                    println!("TCP accept: PCB is in closed state.");
                    return None;
                }
                backlog_id = next_backlog;
                next_backlog = pcb.backlog.pcb_ids.pop_front();
            }
        } else {
            break;
        }
    }
    backlog_id
}

pub fn send(
    pcb_id: usize,
    data: Vec<u8>,
    device: &mut NetDevice,
    contexts: &mut ProtocolContexts,
    pcbs_arc: &mut Arc<Mutex<ControlBlocks>>,
) -> Option<usize> {
    let (sender, receiver) = mpsc::channel();
    let mut sent = 0;
    let mut retry = false;
    let mut pcb_state;
    let mut pcb_send_window;
    let mut pcb_send_next;
    let mut pcb_send_una;
    {
        let pcbs = &mut pcbs_arc.lock().unwrap();
        let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
        pcb.sender = Some(sender);
    }

    loop {
        {
            let pcbs = &mut pcbs_arc.lock().unwrap();
            let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
            pcb_state = pcb.state;
            pcb_send_window = pcb.send_context.window as u32;
            pcb_send_next = pcb.send_context.next;
            pcb_send_una = pcb.send_context.una;
        }
        if pcb_state == TcpPcbState::Closed {
            println!("TCP: connection does not exist.");
            return None;
        } else if pcb_state == TcpPcbState::Listen {
            println!("TCP: this connection is passive.");
            return None;
        } else if pcb_state == TcpPcbState::SynSent || pcb_state == TcpPcbState::SynReceived {
            println!("TCP: insufficient resources.");
            return None;
        } else if pcb_state == TcpPcbState::Established || pcb_state == TcpPcbState::CloseWait {
            let mss = device.mtu - (IP_HEADER_MIN_SIZE + size_of::<TcpHeader>());
            let len = data.len();
            while sent < len {
                let capacity = (pcb_send_window - (pcb_send_next - pcb_send_una)) as usize;
                if capacity < 1 {
                    if !receiver.recv().unwrap() {
                        return None;
                    }
                    retry = true;
                    break;
                } else {
                    let pcbs = &mut pcbs_arc.lock().unwrap();
                    let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
                    let send_len = cmp::min(cmp::min(mss, len - sent), capacity);
                    output(
                        pcb,
                        TcpFlag::ACK as u8 | TcpFlag::PSH as u8,
                        data[send_len..].to_vec(),
                        device,
                        contexts,
                    );
                    pcb.send_context.next += send_len as u32;
                    sent += send_len;
                    retry = false;
                }
            }
            if !retry {
                break;
            }
        } else if pcb_state == TcpPcbState::FinWait1
            || pcb_state == TcpPcbState::FinWait2
            || pcb_state == TcpPcbState::Closing
            || pcb_state == TcpPcbState::LastAck
            || pcb_state == TcpPcbState::TimeWait
        {
            println!("TCP: connection is closing.");
            return None;
        } else {
            println!("TCP: unknown state.");
            return None;
        }
    }
    Some(sent)
}

pub fn receive(pcb_id: usize, size: usize, pcbs_arc: Arc<Mutex<ControlBlocks>>) -> Option<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    let mut remain = 0;
    let mut pcb_state;
    let mut pcb_buf_len;
    let mut pcb_recv_window;
    {
        let pcbs = &mut pcbs_arc.lock().unwrap();
        let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
        pcb.sender = Some(sender);
        pcb_state = pcb.state;
        pcb_buf_len = pcb.buf.len();
        pcb_recv_window = pcb.recv_context.window as usize;
    }

    loop {
        if pcb_state == TcpPcbState::Closed {
            println!("TCP: connection does not exist.");
            return None;
        } else if pcb_state == TcpPcbState::Listen
            || pcb_state == TcpPcbState::SynSent
            || pcb_state == TcpPcbState::SynReceived
        {
            println!("TCP: insufficient resources.");
            return None;
        } else if pcb_state == TcpPcbState::Established
            || pcb_state == TcpPcbState::FinWait1
            || pcb_state == TcpPcbState::FinWait2
        {
            remain = pcb_buf_len - pcb_recv_window;
            if remain > 0 {
                break;
            }
            println!("TCP: receive sleep...");
            if !receiver.recv().unwrap() {
                return None;
            }
            {
                let pcbs = &mut pcbs_arc.lock().unwrap();
                let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
                pcb_state = pcb.state;
                pcb_buf_len = pcb.buf.len();
                pcb_recv_window = pcb.recv_context.window as usize;
            }
        } else if pcb_state == TcpPcbState::CloseWait {
            remain = pcb_buf_len - pcb_recv_window;
            if remain > 0 {
                break;
            }
            break; // fall through
        } else if pcb_state == TcpPcbState::Closing
            || pcb_state == TcpPcbState::LastAck
            || pcb_state == TcpPcbState::TimeWait
        {
            println!("TCP: connection closing.");
        } else {
            println!("TCP: unknown state.");
        }
    }
    let pcbs = &mut pcbs_arc.lock().unwrap();
    let pcb = pcb_by_id(&mut pcbs.tcp_pcbs, pcb_id);
    let len = cmp::min(size, remain);
    let data = pcb.buf[..len].to_vec();
    pcb.buf = pcb.buf[len..].to_vec();
    pcb.recv_context.window += len as u16;
    Some(data)
}

pub fn close(
    pcb_id: usize,
    pcbs: &mut ControlBlocks,
    device: &mut NetDevice,
    contexts: &mut ProtocolContexts,
) {
    let pcb_opt = pcbs.tcp_pcbs.get_mut_by_id(pcb_id);
    if pcb_opt.is_some() {
        let pcb = pcb_opt.unwrap();
        output(pcb, TcpFlag::RST as u8, vec![], device, contexts);
        pcb.release();
    }
}
