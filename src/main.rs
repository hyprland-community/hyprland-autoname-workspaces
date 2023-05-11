mod config;
mod params;
mod renamer;

use crate::config::Config;
use crate::params::Args;
use crate::renamer::*;

use clap::Parser;
use file_lock::{FileLock, FileOptions};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::{process, thread};

fn main() {
    let cfg = Config::new().expect("Unable to read config");
    let args = Args::parse();

    if args.dump {
        println!("{:#?}", &cfg);
        process::exit(0);
    }

    // Ensure only one instance running
    let lock = get_lock();

    // Init
    let renamer = Renamer::new(cfg, args);
    renamer
        .renameworkspace()
        .expect("App can't rename workspaces on start");

    // Handle unix signals
    let mut signals = Signals::new([SIGINT, SIGTERM]).expect("Can't listen on SIGINT or SIGTERM");
    let final_renamer = renamer.clone();
    thread::spawn(move || {
        for _ in signals.forever() {
            match final_renamer.reset_workspaces() {
                Err(_) => println!("Workspaces name can't be cleared"),
                Ok(_) => println!("Workspaces name cleared, bye"),
            };
            _ = lock.unlock();
            process::exit(0);
        }
    });

    let config_renamer = renamer.clone();
    thread::spawn(move || {
        config_renamer
            .watch_config_changes()
            .expect("Unable to watch for config changes")
    });

    renamer
        .start_listeners()
        .expect("Can't listen Hyprland events on reload, sorry");
}

fn get_lock() -> FileLock {
    match FileLock::lock(
        "/tmp/hyprland-autoname-workspaces.lock",
        false,
        FileOptions::new().write(true).create(true).append(true),
    ) {
        Ok(lock) => lock,
        Err(_) => {
            let app = prog().unwrap();
            eprintln!("The program {app} is already running, bye");
            process::exit(1);
        }
    }
}

fn prog() -> Option<String> {
    std::env::current_exe()
        .ok()?
        .file_name()?
        .to_str()?
        .to_owned()
        .into()
}
