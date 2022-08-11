mod app;
mod device;
mod interrupt;

use std::io::Error;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use signal_hook::consts::signal::*;
use signal_hook::consts::TERM_SIGNALS;
use signal_hook::iterator::exfiltrator::origin::WithOrigin;
use signal_hook::iterator::SignalsInfo;

fn main() -> Result<(), Error> {
    let mut sigs = vec![SIGHUP, SIGUSR1, interrupt::INTR_IRQ_NULL];
    sigs.extend(TERM_SIGNALS);
    let mut signals = SignalsInfo::<WithOrigin>::new(&sigs)?;

    let (sender, receiver) = mpsc::channel();
    app::run(receiver);

    for info in &mut signals {
        eprintln!("Received a signal {:?}", info);
        match info.signal {
            SIGHUP => {}
            SIGUSR1 => {}
            interrupt::INTR_IRQ_NULL => {
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
    thread::sleep(Duration::from_millis(1000));
    Ok(())
}
