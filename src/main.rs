mod devices;
mod drivers;
mod interrupt;
mod net;
mod protocol_stack;
mod protocols;
mod util;

use crate::devices::ethernet::IRQ_ETHERNET;
use crate::devices::loopback::IRQ_LOOPBACK;
use crate::protocol_stack::ProtoStackSetup;
use signal_hook::consts::signal::*;
use signal_hook::consts::TERM_SIGNALS;
use signal_hook::iterator::exfiltrator::origin::WithOrigin;
use signal_hook::iterator::SignalsInfo;
use std::io::Error;
use std::sync::mpsc;

fn main() -> Result<(), Error> {
    // Signal setup
    let mut sigs = vec![SIGHUP, SIGUSR1, IRQ_LOOPBACK, IRQ_ETHERNET];
    sigs.extend(TERM_SIGNALS);
    let mut signals = SignalsInfo::<WithOrigin>::new(&sigs)?;

    let (sender, receiver) = mpsc::channel();

    // Protocol stack start
    let proto_stack = ProtoStackSetup::new();
    let app_join = proto_stack.run(receiver);

    // Interrupt thread
    println!("Starting signal receiver thread...");
    for info in &mut signals {
        eprintln!("Received a signal {:?}", info);
        match info.signal {
            SIGHUP => {}
            SIGUSR1 => {
                proto_stack.handle_protocol();
            }
            sig => {
                if TERM_SIGNALS.contains(&sig) {
                    eprintln!("Terminating");
                    break;
                }
                proto_stack.handle_irq(sig);
            }
        }
    }
    // App thread termination
    println!("Closing app thread.");
    sender.send(()).unwrap();
    app_join.join().unwrap();
    println!("App thread closed.");
    Ok(())
}
