mod config;
mod params;
mod renamer;

use crate::config::Config;
use crate::params::Args;
use crate::renamer::*;

use clap::Parser;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use single_instance::SingleInstance;
use std::{process, thread};

fn main() {
    let cfg = Config::new().expect("Unable to read config");
    let args = Args::parse();

    if args.dump {
        println!("{:#?}", &cfg);
        process::exit(0);
    }

    let instance = SingleInstance::new("Hyprland-autoname-workspaces").unwrap();
    if !instance.is_single() {
        eprintln!("Hyprland-autoname-workspaces is already running, exit");
        process::exit(1);
    }

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
