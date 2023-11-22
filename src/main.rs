mod config;
mod params;
mod renamer;

use crate::config::Config;
use crate::params::Args;
use crate::renamer::*;

use clap::Parser;
use config::get_config_path;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use single_instance::SingleInstance;
use std::{process, thread};

fn main() {
    let args = Args::parse();
    let cfg_path = get_config_path(&args.config).expect("Can't get config path");
    let cfg = Config::new(cfg_path, args.dump, args.migrate_config).expect("Unable to read config");

    let instance = SingleInstance::new("Hyprland-autoname-workspaces").unwrap();
    if !instance.is_single() {
        eprintln!("Hyprland-autoname-workspaces is already running, exit");
        process::exit(1);
    }

    // Init
    let renamer = Renamer::new(cfg.clone(), args);
    renamer
        .rename_workspace()
        .expect("App can't rename workspaces on start");

    // Handle unix signals
    let mut signals = Signals::new([SIGINT, SIGTERM]).expect("Can't listen on SIGINT or SIGTERM");
    let final_renamer = renamer.clone();

    thread::spawn(move || {
        if signals.forever().next().is_some() {
            match final_renamer.reset_workspaces(cfg.config) {
                Err(_) => println!("Workspaces name can't be cleared"),
                Ok(_) => println!("Workspaces name cleared, bye"),
            };
            process::exit(0);
        }
    });

    let config_renamer = renamer.clone();
    thread::spawn(move || {
        config_renamer
            .watch_config_changes(cfg.cfg_path)
            .expect("Unable to watch for config changes")
    });

    renamer.start_listeners()
}
