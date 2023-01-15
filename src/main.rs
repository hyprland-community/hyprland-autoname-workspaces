use clap::Parser;
use core::str;
use hyprland::data::Clients;
use hyprland::dispatch::*;
use hyprland::event_listener::EventListenerMutable as EventListener;
use hyprland::prelude::*;
use hyprland::shared::{HResult, WorkspaceType};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::{process, thread};

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    dedup: bool,
}

struct Config {
    icons: HashMap<String, String>,
}

impl Config {
    fn new() -> Config {
        let xdg_dirs = xdg::BaseDirectories::with_prefix("hyrland-autoname-workspaces").unwrap();
        let mut cfg_path = xdg_dirs.find_config_file("config.toml");
        cfg_path = match cfg_path.clone() {
            None => {
                let config_path = xdg_dirs
                    .place_config_file("config.toml")
                    .expect("cannot create configuration directory");
                let mut config_file = File::create(config_path.clone()).unwrap();
                let default_icons = r#"# Add your icons mapping
DEFAULT = ""
kitty = "term"
firefox = "browser"
            "#;
                write!(&mut config_file, "{}", default_icons).unwrap();
                println!("Default config created in {:?}", config_path);
                xdg_dirs.find_config_file("config.toml")
            }
            Some(_) => cfg_path,
        };
        let config =
            fs::read_to_string(cfg_path.unwrap()).expect("Should have been able to read the file");
        let icons: HashMap<String, String> = toml::from_str(&config).unwrap();
        Config { icons }
    }
}

fn main() -> HResult<()> {
    let cfg = Config::new();
    // Init
    let renamer = Arc::new(Renamer::new(cfg, Args::parse()));
    renamer.renameworkspace();

    // Handle unix signals
    let mut signals = Signals::new([SIGINT, SIGTERM])?;
    let final_renamer = renamer.clone();
    thread::spawn(move || {
        for _ in signals.forever() {
            final_renamer.reset_workspaces();
            process::exit(0);
        }
    });

    // Run on window events
    renamer.start_listeners()
}

struct Renamer {
    workspaces: Mutex<HashSet<i32>>,
    cfg: Config,
    args: Args,
}

impl Renamer {
    fn new(cfg: Config, args: Args) -> Self {
        let workspaces = Mutex::new(HashSet::new());
        Renamer {
            workspaces,
            cfg,
            args,
        }
    }

    fn removeworkspace(&self, wt: WorkspaceType) {
        match wt {
            WorkspaceType::Unnamed(x) => self.workspaces.lock().unwrap().remove(&x),
            WorkspaceType::Special(_) => false,
            WorkspaceType::Named(_) => false,
        };
    }

    fn renameworkspace(&self) {
        let clients = Clients::get().unwrap();
        let mut deduper: HashSet<String> = HashSet::new();
        let mut workspaces = self
            .workspaces
            .lock()
            .unwrap()
            .iter()
            .map(|&c| (c, "".to_string()))
            .collect::<HashMap<_, _>>();

        for client in clients.collect().iter() {
            let class = client.clone().class.to_lowercase();
            let fullscreen = client.fullscreen;
            let icon = self.class_to_icon(&class).to_string();
            let workspace_id = client.clone().workspace.id;
            let is_dup = !deduper.insert(format!("{}-{}", workspace_id.clone(), icon));
            let should_dedup = self.args.dedup && is_dup;

            self.workspaces
                .lock()
                .unwrap()
                .insert(client.clone().workspace.id);

            let workspace = workspaces.entry(workspace_id).or_insert("".to_string());

            if fullscreen && should_dedup {
                *workspace = workspace.replace(&icon, &format!("[{}]", &icon));
            } else if fullscreen && !should_dedup {
                *workspace = format!("{} [{}]", workspace, icon);
            } else if !should_dedup {
                *workspace = format!("{} {}", workspace, icon);
            }
        }

        for (id, apps) in workspaces.clone().into_iter() {
            rename_cmd(id, &apps);
        }
    }

    fn reset_workspaces(&self) {
        for &id in self.workspaces.lock().unwrap().iter() {
            rename_cmd(id, "");
        }
    }

    fn start_listeners(self: &Arc<Self>) -> HResult<()> {
        let mut event_listener = EventListener::new();

        let this = self.clone();
        event_listener.add_window_open_handler(move |_, _| this.renameworkspace());
        let this = self.clone();
        event_listener.add_window_moved_handler(move |_, _| this.renameworkspace());
        let this = self.clone();
        event_listener.add_window_close_handler(move |_, _| this.renameworkspace());
        let this = self.clone();
        event_listener.add_workspace_added_handler(move |_, _| this.renameworkspace());
        let this = self.clone();
        event_listener.add_workspace_moved_handler(move |_, _| this.renameworkspace());
        let this = self.clone();
        event_listener.add_workspace_change_handler(move |_, _| this.renameworkspace());
        let this = self.clone();
        event_listener.add_fullscreen_state_change_handler(move |_, _| this.renameworkspace());
        let this = self.clone();
        event_listener.add_workspace_destroy_handler(move |wt, _| {
            this.renameworkspace();
            this.removeworkspace(wt);
        });

        event_listener.start_listener()
    }

    fn class_to_icon(&self, class: &str) -> &str {
        return self
            .cfg
            .icons
            .get(class)
            .unwrap_or_else(|| self.cfg.icons.get("DEFAULT").unwrap());
    }
}

fn rename_cmd(id: i32, apps: &str) {
    let text = format!("{}:{}", id.clone(), apps);
    let content = (!apps.is_empty()).then_some(text.as_str());
    hyprland::dispatch!(RenameWorkspace, id, content).unwrap();
}
