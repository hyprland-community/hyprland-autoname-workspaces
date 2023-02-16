use clap::Parser;
use hyprland::data::Clients;
use hyprland::dispatch::*;
use hyprland::event_listener::EventListenerMutable as EventListener;
use hyprland::prelude::*;
use hyprland::shared::WorkspaceType;
use inotify::{Inotify, WatchMask};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::{process, thread};

#[derive(Parser)]
struct Args {
    /// Deduplicate icons
    #[arg(short, long)]
    dedup: bool,
}

struct Config {
    icons: HashMap<String, String>,
    cfg_path: PathBuf,
}

impl Config {
    fn new(_args: &Args) -> Result<Config, Box<dyn Error>> {
        let mut config_file: File;
        let xdg_dirs = xdg::BaseDirectories::with_prefix("hyprland-autoname-workspaces")?;
        let cfg_path = xdg_dirs.place_config_file("config.toml")?;
        if !cfg_path.exists() {
            config_file = File::create(&cfg_path)?;
            let default_icons = r#"# Add your icons mapping
# use double quote the key and the value
# take class name from 'hyprctl clients'
"DEFAULT" = ""
"kitty" = "term"
"firefox" = "browser"
            "#;
            write!(&mut config_file, "{default_icons}")?;
            println!("Default config created in {cfg_path:?}");
        }
        let config = fs::read_to_string(cfg_path.clone())?;
        let icons: HashMap<String, String> =
            toml::from_str(&config).map_err(|e| format!("Unable to parse: {e:?}"))?;
        let icons_uppercase = icons
            .iter()
            .map(|(k, v)| (k.to_uppercase(), v.clone()))
            .collect::<HashMap<_, _>>();
        let icons = icons.into_iter().chain(icons_uppercase).collect();
        Ok(Config { cfg_path, icons })
    }
}

fn main() {
    let args = Args::parse();
    let cfg = Config::new(&args).expect("Unable to read config");

    // Init
    let renamer = Arc::new(Renamer::new(cfg, args));
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

struct Renamer {
    workspaces: Mutex<HashSet<i32>>,
    cfg: Mutex<Config>,
    args: Args,
}

impl Renamer {
    fn new(cfg: Config, args: Args) -> Self {
        Renamer {
            workspaces: Mutex::new(HashSet::new()),
            cfg: Mutex::new(cfg),
            args,
        }
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

        for client in clients {
            let class = client.class;
            if class.is_empty() {
                continue;
            }
            let workspace_id = client.workspace.id;
            let icon = self.class_to_icon(&class);
            let fullscreen = client.fullscreen;
            let is_dup = !deduper.insert(format!("{workspace_id}-{icon}"));
            let should_dedup = self.args.dedup && is_dup;

            self.workspaces.lock()?.insert(client.workspace.id);

            let workspace = workspaces
                .entry(workspace_id)
                .or_insert_with(|| "".to_string());

            if fullscreen && should_dedup {
                *workspace = workspace.replace(&icon, &format!("[{icon}]"));
            } else if fullscreen && !should_dedup {
                *workspace = format!("{workspace} [{icon}]");
            } else if !should_dedup {
                *workspace = format!("{workspace} {icon}");
            }
        }

        workspaces
            .iter()
            .try_for_each(|(&id, apps)| rename_cmd(id, apps))?;

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

    fn watch_config_changes(&self) -> Result<(), Box<dyn Error + '_>> {
        loop {
            // Watch for modify events.
            let mut notify = Inotify::init()?;

            notify.add_watch(&self.cfg.lock()?.cfg_path, WatchMask::MODIFY)?;
            let mut buffer = [0; 1024];
            notify.read_events_blocking(&mut buffer)?.last();

            println!("Reloading config !");
            // Clojure to force quick release of lock
            {
                match Config::new(&self.args) {
                    Ok(config) => self.cfg.lock()?.icons = config.icons,
                    Err(err) => println!("Unable to reload config: {err:?}"),
                }
            }

            // Handle event
            // Run on window events
            _ = self.renameworkspace();
        }
    }

    fn class_to_icon(&self, class: &str) -> String {
        let default_value = String::from("no default icon");
        let cfg = self.cfg.lock().expect("Unable to obtain lock for config");
        cfg.icons
            .get(class)
            .or_else(|| cfg.icons.get(class.to_uppercase().as_str()))
            .unwrap_or_else(|| cfg.icons.get("DEFAULT").unwrap_or(&default_value))
            .into()
    }

    fn removeworkspace(&self, wt: WorkspaceType) -> Result<(), Box<dyn Error + '_>> {
        match wt {
            WorkspaceType::Regular(x) => self.workspaces.lock()?.remove(&x.parse::<i32>()?),
            WorkspaceType::Special(_) => false,
        };

        Ok(())
    }
}

fn rename_cmd(id: i32, apps: &str) -> Result<(), Box<dyn Error>> {
    let text = format!("{id}:{apps}");
    let content = (!apps.is_empty()).then_some(text.as_str());
    hyprland::dispatch!(RenameWorkspace, id, content)?;

    Ok(())
}
