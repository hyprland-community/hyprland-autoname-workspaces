use clap::Parser;
use hyprland::data::Clients;
use hyprland::dispatch::*;
use hyprland::event_listener::EventListenerMutable as EventListener;
use hyprland::prelude::*;
use hyprland::shared::WorkspaceType;
use inotify::{Inotify, WatchMask};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
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
    config: ConfigFile,
    cfg_path: PathBuf,
}

#[derive(Deserialize)]
struct ConfigFile {
    icons: FxHashMap<String, String>,
    #[serde(default)]
    exclude: FxHashMap<String, String>,
}

impl Config {
    fn new() -> Result<Config, Box<dyn Error>> {
        let mut config_file: File;
        let xdg_dirs = xdg::BaseDirectories::with_prefix("hyprland-autoname-workspaces")?;
        let cfg_path = xdg_dirs.place_config_file("config.toml")?;
        if !cfg_path.exists() {
            config_file = File::create(&cfg_path)?;
            let default_config = r#"[icons]
# Add your icons mapping
# use double quote the key and the value
# take class name from 'hyprctl clients'
"DEFAULT" = ""
"kitty" = "term"
"firefox" = "browser"

# Add your applications that need to be exclude
# You can put what you want as value, "" make the job.
[exclude]
fcitx5 = ""
fcitx = ""
"#;
            write!(&mut config_file, "{default_config}")?;
            println!("Default config created in {cfg_path:?}");
        }
        let mut config_string = fs::read_to_string(cfg_path.clone())?;

        // config file migration if needed
        // can be remove "later" ...
        if !config_string.contains("[icons]") {
            config_string = "[icons]\n".to_owned() + &config_string;
            fs::write(&cfg_path, &config_string)
                .map_err(|e| format!("Cannot migrate config file: {e:?}"))?;
            println!("Config file migrated from v1 to v2");
        }

        let config: ConfigFile =
            toml::from_str(&config_string).map_err(|e| format!("Unable to parse: {e:?}"))?;

        let icons = config
            .icons
            .iter()
            .map(|(k, v)| (k.to_uppercase(), v.clone()))
            .collect::<FxHashMap<_, _>>();

        let exclude = config
            .exclude
            .iter()
            .map(|(k, v)| (k.to_uppercase(), v.clone()))
            .collect::<FxHashMap<_, _>>();

        Ok(Config {
            config: ConfigFile { icons, exclude },
            cfg_path,
        })
    }
}

fn main() {
    let cfg = Config::new().expect("Unable to read config");

    // Init
    let renamer = Arc::new(Renamer::new(cfg, Args::parse()));
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
    workspaces: Mutex<FxHashSet<i32>>,
    cfg: Mutex<Config>,
    args: Args,
}

impl Renamer {
    fn new(cfg: Config, args: Args) -> Self {
        Renamer {
            workspaces: Mutex::new(FxHashSet::default()),
            cfg: Mutex::new(cfg),
            args,
        }
    }

    #[inline(always)]
    fn renameworkspace(&self) -> Result<(), Box<dyn Error + '_>> {
        let clients = Clients::get().unwrap();
        let mut deduper: FxHashSet<String> = FxHashSet::default();
        let mut workspaces = self
            .workspaces
            .lock()?
            .iter()
            .map(|&c| (c, "".to_string()))
            .collect::<FxHashMap<_, _>>();

        for client in clients {
            let class = client.class;

            if class.is_empty() {
                continue;
            }

            if self
                .cfg
                .lock()?
                .config
                .exclude
                .contains_key(&class.to_uppercase())
            {
                continue;
            }

            let workspace_id = client.workspace.id;
            let icon = self.class_to_icon(&class);
            let is_dup = !deduper.insert(format!("{workspace_id}-{icon}"));
            let should_dedup = self.args.dedup && is_dup;

            self.workspaces.lock()?.insert(workspace_id);

            let workspace = workspaces
                .entry(workspace_id)
                .or_insert_with(|| "".to_string());

            if client.fullscreen && should_dedup {
                *workspace = workspace.replace(&icon, &format!("[{icon}]"));
            } else if client.fullscreen && !should_dedup {
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

        rename_workspace_if!(
            self,
            event_listener,
            add_window_open_handler,
            add_window_close_handler,
            add_window_moved_handler,
            add_workspace_added_handler,
            add_workspace_moved_handler,
            add_workspace_change_handler,
            add_fullscreen_state_change_handler
        );

        let this = self.clone();
        event_listener.add_workspace_destroy_handler(move |wt, _| {
            _ = this.renameworkspace();
            _ = this.removeworkspace(wt);
        });

        event_listener.start_listener()?;

        Ok(())
    }

    #[inline(always)]
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
                match Config::new() {
                    Ok(config) => self.cfg.lock()?.config = config.config,
                    Err(err) => println!("Unable to reload config: {err:?}"),
                }
            }

            // Handle event
            // Run on window events
            _ = self.renameworkspace();
        }
    }

    #[inline(always)]
    fn class_to_icon(&self, class: &str) -> String {
        let default_value = String::from("no default icon");
        let cfg = &self.cfg.lock().expect("Unable to obtain lock for config");
        cfg.config
            .icons
            .get(class)
            .or_else(|| cfg.config.icons.get(class.to_uppercase().as_str()))
            .unwrap_or_else(|| cfg.config.icons.get("DEFAULT").unwrap_or(&default_value))
            .into()
    }

    #[inline(always)]
    fn removeworkspace(&self, wt: WorkspaceType) -> Result<(), Box<dyn Error + '_>> {
        match wt {
            WorkspaceType::Regular(x) => self.workspaces.lock()?.remove(&x.parse::<i32>()?),
            WorkspaceType::Special(_) => false,
        };

        Ok(())
    }
}

#[inline(always)]
fn rename_cmd(id: i32, apps: &str) -> Result<(), Box<dyn Error>> {
    let text = format!("{id}:{apps}");
    let content = (!apps.is_empty()).then_some(text.as_str());
    hyprland::dispatch!(RenameWorkspace, id, content)?;

    Ok(())
}

#[macro_export]
macro_rules! rename_workspace_if{
    ( $self: ident, $ev: ident, $( $x:ident ), * ) => {
        $(
        let this = $self.clone();
        $ev.$x(move |_, _| _ = this.renameworkspace());
        )*
    };
}
