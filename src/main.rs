use anyhow::Result;
use clap::Parser;
use core::str;
use hyprland::data::Clients;
use hyprland::dispatch::*;
use hyprland::event_listener::EventListenerMutable as EventListener;
use hyprland::prelude::*;
use hyprland::shared::WorkspaceType;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::collections::{HashMap, HashSet};
use std::error::Error;
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
        let xdg_dirs = xdg::BaseDirectories::with_prefix("hyprland-autoname-workspaces").unwrap();
        let cfg_path = xdg_dirs
            .place_config_file("config.toml")
            .expect("Cannot create configuration directory");
        if !cfg_path.exists() {
            let mut config_file = File::create(&cfg_path).expect("Can't create config dir");
            let default_icons = r#"# Add your icons mapping
# Take care to lowercase app name
# and use double quote the key and the value
"DEFAULT" = ""
"kitty" = "term"
"firefox" = "browser"
            "#;
            write!(&mut config_file, "{}", default_icons).expect("Can't write default config file");
            println!("Default config created in {:?}", cfg_path);
        }
        let config = fs::read_to_string(cfg_path).expect("Should have been able to read the file");
        let icons: HashMap<String, String> =
            toml::from_str(&config).expect("Can't read config file");
        Config { icons }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let cfg = Config::new();
    // Init
    let renamer = Arc::new(Renamer::new(cfg, Args::parse()));
    _ = renamer.renameworkspace();

    // Handle unix signals
    let mut signals = Signals::new([SIGINT, SIGTERM])?;
    let final_renamer = renamer.clone();
    thread::spawn(move || {
        for _ in signals.forever() {
            _ = final_renamer.reset_workspaces();
            process::exit(0);
        }
    });

    // Run on window events
    renamer.start_listeners()?;

    Ok(())
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

    fn removeworkspace(&self, wt: WorkspaceType) -> Result<(), Box<dyn Error + '_>> {
        match wt {
            WorkspaceType::Regular(x) => self.workspaces.lock()?.remove(&x.parse::<i32>()?),
            WorkspaceType::Special(_) => false,
        };

        Ok(())
    }

    fn renameworkspace(&self) -> Result<(), Box<dyn Error + '_>> {
        let clients = Clients::get().unwrap();
        let mut deduper: HashSet<String> = HashSet::new();
        let mut workspaces = self
            .workspaces
            .lock()?
            .iter()
            .map(|&c| (c, "".to_string()))
            .collect::<HashMap<_, _>>();

        for client in clients.into_iter() {
            let class = client.clone().class.to_lowercase();
            let fullscreen = client.fullscreen;
            let icon = self.class_to_icon(&class);
            let workspace_id = client.clone().workspace.id;
            let is_dup = !deduper.insert(format!("{}-{}", workspace_id.clone(), icon));
            let should_dedup = self.args.dedup && is_dup;

            self.workspaces.lock()?.insert(client.clone().workspace.id);

            let workspace = workspaces.entry(workspace_id).or_insert("".to_string());

            if fullscreen && should_dedup {
                *workspace = workspace.replace(&icon, &format!("[{}]", &icon));
            } else if fullscreen && !should_dedup {
                *workspace = format!("{} [{}]", workspace, icon);
            } else if !should_dedup {
                *workspace = format!("{} {}", workspace, icon);
            }
        }

        workspaces
            .clone()
            .iter()
            .try_for_each(|(&id, apps)| rename_cmd(id, &apps.clone()))?;

        Ok(())
    }

    fn reset_workspaces(&self) -> Result<(), Box<dyn Error + '_>> {
        self.workspaces
            .lock()?
            .iter()
            .try_for_each(|&id| rename_cmd(id, ""))
    }

    fn start_listeners(self: &Arc<Self>) -> Result<(), Box<dyn Error>> {
        let mut event_listener = EventListener::new();

        let this = self.clone();
        event_listener.add_window_open_handler(move |_, _| _ = this.renameworkspace());
        let this = self.clone();
        event_listener.add_window_moved_handler(move |_, _| _ = this.renameworkspace());
        let this = self.clone();
        event_listener.add_window_close_handler(move |_, _| _ = this.renameworkspace());
        let this = self.clone();
        event_listener.add_workspace_added_handler(move |_, _| _ = this.renameworkspace());
        let this = self.clone();
        event_listener.add_workspace_moved_handler(move |_, _| _ = this.renameworkspace());
        let this = self.clone();
        event_listener.add_workspace_change_handler(move |_, _| _ = this.renameworkspace());
        let this = self.clone();
        event_listener.add_fullscreen_state_change_handler(move |_, _| _ = this.renameworkspace());
        let this = self.clone();
        event_listener.add_workspace_destroy_handler(move |wt, _| {
            _ = this.renameworkspace();
            _ = this.removeworkspace(wt);
        });

        event_listener.start_listener()?;

        Ok(())
    }

    fn class_to_icon(&self, class: &str) -> String {
        let default_value = String::from("no default icon");
        self.cfg
            .icons
            .get(class)
            .unwrap_or_else(|| {
                self.cfg
                    .icons
                    .get("DEFAULT")
                    .unwrap_or_else(|| &default_value)
            })
            .into()
    }
}

fn rename_cmd(id: i32, apps: &str) -> Result<(), Box<dyn Error>> {
    let text = format!("{}:{}", id.clone(), apps);
    let content = (!apps.is_empty()).then_some(text.as_str());
    hyprland::dispatch!(RenameWorkspace, id, content)?;

    Ok(())
}
