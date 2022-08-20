mod app;
mod device;
mod interrupt;
mod protocol;

use std::io::Error;
use std::sync::mpsc;

use signal_hook::consts::signal::*;
use signal_hook::consts::TERM_SIGNALS;
use signal_hook::iterator::exfiltrator::origin::WithOrigin;
use signal_hook::iterator::SignalsInfo;

use crate::app::ProtoStackSetup;

fn main() -> Result<(), Error> {
    // Interrupt thread
    let mut sigs = vec![SIGHUP, SIGUSR1, device::IRQ_LOOPBACK];
    sigs.extend(TERM_SIGNALS);
    let mut signals = SignalsInfo::<WithOrigin>::new(&sigs)?;

    let (sender, receiver) = mpsc::channel();

    let proto_stack = ProtoStackSetup::new();
    let app_join = proto_stack.run(receiver);

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
    // Stop app thread
    println!("Closing app thread.");
    sender.send(()).unwrap();
    app_join.join().unwrap();
    println!("App thread closed.");
    Ok(())
}
