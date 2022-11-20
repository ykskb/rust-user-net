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
use log::debug;
use log::info;
use signal_hook::consts::signal::*;
use signal_hook::consts::TERM_SIGNALS;
use signal_hook::iterator::exfiltrator::origin::WithOrigin;
use signal_hook::iterator::SignalsInfo;
use simplelog::Config;
use simplelog::SimpleLogger;
use std::io::Error;
use std::sync::mpsc;

fn main() -> Result<(), Error> {
    // Signal setup
    let mut sigs = vec![SIGHUP, SIGUSR1, IRQ_LOOPBACK, IRQ_ETHERNET];
    sigs.extend(TERM_SIGNALS);
    let mut signals = SignalsInfo::<WithOrigin>::new(&sigs)?;

    // Log setup
    SimpleLogger::init(log::LevelFilter::Info, Config::default()).unwrap();

    let (app_sender, app_receiver) = mpsc::channel();
    let (tcp_sender, tcp_receiver) = mpsc::channel();

    // Protocol stack start
    let mut app = NetApp::new();
    let app_join = app.run(app_receiver);
    let tcp_join = app.tcp_transmit_thread(tcp_receiver);

    // Interrupt thread
    info!("App: starting signal receiver thread...");
    for info in &mut signals {
        debug!("App: ----Signal Received {:?}----\n", info);
        match info.signal {
            SIGHUP => {}
            SIGUSR1 => {
                app.handle_protocol();
            }
            sig => {
                if TERM_SIGNALS.contains(&sig) {
                    info!("Terminating");
                    break;
                }
                app.handle_irq(sig);
            }
        }
    }
    info!("App: closing app/TCP retransmission thread...");
    app_sender.send(()).unwrap();
    tcp_sender.send(()).unwrap();
    app.close_sockets();
    app_join.join().unwrap();
    tcp_join.join().unwrap();
    info!("App: closed.");
    Ok(())
}
