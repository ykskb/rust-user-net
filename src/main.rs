mod app;
mod device;
mod interrupt;
mod net;
mod protocol;

use std::io::Error;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

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
    let setup = ProtoStackSetup::new();
    let app_join = setup.run(receiver);

    for info in &mut signals {
        eprintln!("Received a signal {:?}", info);
        match info.signal {
            SIGHUP => {}
            SIGUSR1 => {
                net::handle_soft_irq();
            }
            device::IRQ_LOOPBACK => {
                eprint!("yahoo");
            }
            term_sig => {
                eprintln!("Terminating");
                assert!(TERM_SIGNALS.contains(&term_sig));
                break;
            }
        }
    }
    // Stop app thread
    println!("Closing app thread.");
    sender.send(()).unwrap();
    app_join.join();
    println!("App thread closed.");
    Ok(())
}
