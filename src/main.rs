mod app;
mod devices;
mod drivers;
mod interrupt;
mod net;
mod protocols;
mod util;

use crate::app::NetApp;
use crate::devices::ethernet::IRQ_ETHERNET;
use crate::devices::loopback::IRQ_LOOPBACK;
use signal_hook::consts::signal::*;
use signal_hook::consts::TERM_SIGNALS;
use signal_hook::iterator::exfiltrator::origin::WithOrigin;
use signal_hook::iterator::SignalsInfo;
use std::io::Error;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Error> {
    // Signal setup
    let mut sigs = vec![SIGHUP, SIGUSR1, IRQ_LOOPBACK, IRQ_ETHERNET];
    sigs.extend(TERM_SIGNALS);
    let mut signals = SignalsInfo::<WithOrigin>::new(&sigs)?;

    let (sender, receiver) = mpsc::channel();

    // Protocol stack start
    let mut app = NetApp::new();
    let app_join = app.run(receiver);

    // Interrupt thread
    println!("Starting signal receiver thread...");
    for info in &mut signals {
        eprintln!("========== Received a signal {:?}\n", info);
        match info.signal {
            SIGHUP => {}
            SIGUSR1 => {
                app.handle_protocol();
            }
            sig => {
                if TERM_SIGNALS.contains(&sig) {
                    eprintln!("Terminating");
                    break;
                }
                app.handle_irq(sig);
            }
        }
    }
    // App thread termination
    println!("Closing app thread.");
    sender.send(()).unwrap();
    app.close_sockets();
    app_join.join().unwrap();
    println!("App thread closed.");
    Ok(())
}
