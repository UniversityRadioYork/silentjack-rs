extern crate clap;
extern crate jack;

use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

use clap::{App, Arg};

// db2lin(-90.0)
const MINUS_90_DB: f32 = 0.00003162327766f32;

#[inline]
fn lin2db(lin: f32) -> f32 {
    // println!("lin {}", lin);
    if lin <= MINUS_90_DB {
        return -90.0;
    }
    return 20.0 * lin.log10();
}

#[inline]
fn db2lin(db: f32) -> f32 {
    if db <= -90.0 {
        return 0.0;
    }
    return 10.0f32.powf(db * 0.05);
}

fn main() {
    let args = App::new("Silentjack RS")
        .version("1.0")
        .author("URY / Marks Polakovs")
        .arg(
            Arg::with_name("threshold")
                .help("The level at which the silence alarm will trigger, in negative dB")
                .short("l")
                .long("threshold")
                .takes_value(true)
                .default_value("-40.0"),
        )
        .arg(
            Arg::with_name("timeout")
                .help("For how many seconds there must be silence before the alarm will trigger")
                .short("t")
                .long("timeout")
                .takes_value(true)
                .default_value("45"),
        )
        .arg(
            Arg::with_name("command")
                .help("The command to run when silence is detected")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("port")
                .help("The JACK input port to automatically connect to")
                .short("p")
                .long("port")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .multiple(true)
                .help("Level of verbosity of debug output"),
        )
        .get_matches();

    let verbosity = args.occurrences_of("verbose");

    let (client, status) = jack::Client::new("silentjack-rs", jack::ClientOptions::NO_START_SERVER)
        .expect("Jack client startup failed! Examine the error information.");

    println!("Startup status: {:?}", status);

    let in_port = client
        .register_port("silent_in", jack::AudioIn::default())
        .unwrap();
    let in_port_name = in_port.name().unwrap();

    let peak = Arc::new(Mutex::new(-90.0));

    let peak1 = peak.clone();
    let process_handler = move |_: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
        let in_data = in_port.as_slice(ps);

        let mut peak_val = peak1.lock().unwrap();

        for sample in in_data.iter() {
            // stupid nonsense
            // TODO: might not be that stupid
            if *sample == -1.0000001 || *sample == 1.0 {
                continue;
            }
            let val = lin2db((*sample).abs());
            // println!("{}: {}", *sample, val);
            if val > *peak_val { // they're all negative
                // println!("new peak {}, linear scale {}", val, *sample);
                *peak_val = val;
            }
        }

        jack::Control::Continue
    };

    // let jack_shutting_down = Arc::new(Mutex::new(false));

    // struct SilentjackNotifHandler;
    // impl jack::NotificationHandler for SilentjackNotifHandler {
    //     fn shutdown(&mut self,_status: ClientStatus, _reason: &str) {
    //         *(jack_shutting_down.clone().lock().unwrap()) = true;
    //     }
    // }

    let active_client = client
        .activate_async((), jack::ClosureProcessHandler::new(process_handler))
        .unwrap();

    if let Some(s) = args.value_of("port") {
        let cl = active_client.as_client();

        cl.connect_ports(
            &cl.port_by_name(s).expect("No such port exists!"),
            &cl.port_by_name(&in_port_name).unwrap(),
        )
        .unwrap();
    }

    let silence_threshold: f32 = args
        .value_of("threshold")
        .unwrap()
        .parse::<f32>()
        .expect("Invalid value for threshold!");
    let silence_timeout: i32 = args
        .value_of("timeout")
        .unwrap()
        .parse::<i32>()
        .expect("Invalid value for timeout (integer please)!");
    let silence_command: &str = args.value_of("command").unwrap();

    let mut silent: bool = false;
    let mut silence_seconds: i32 = 0;

    println!("Running.");

    let peak2 = peak.clone();
    loop {
        {
            let mut peak_val = peak2.lock().unwrap();
            if verbosity > 0 {
                if verbosity > 1 || silence_seconds > 0 {
                    println!("peak value: {}", *peak_val);
                }
            }
            if *peak_val < silence_threshold {
                silence_seconds += 1;
                println!("{} seconds of silence!", silence_seconds);
                if silence_seconds > silence_timeout {
                    if !silent {
                        if let Err(ret) = Command::new(silence_command).output() {
                            println!("Executing silence command failed! {}", ret);
                        }
                    }
                    silent = true;
                }
            } else {
                if silent || silence_seconds > 0 {
                    println!("silence over.");
                }
                silence_seconds = 0;
                silent = false;
            }
            *peak_val = -90.0;
        }
        thread::sleep(std::time::Duration::from_secs(1));
    }
}
