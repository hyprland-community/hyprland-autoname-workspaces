use crate::config::Config;
use crate::params::Args;
use hyprland::data::Clients;
use hyprland::dispatch::*;
use hyprland::event_listener::EventListenerMutable as EventListener;
use hyprland::prelude::*;
use hyprland::shared::WorkspaceType;
use inotify::{Inotify, WatchMask};
use rustc_hash::{FxHashMap, FxHashSet};
use std::error::Error;
use std::sync::{Arc, Mutex};
#[macro_use]
mod macros;
mod icons;

use icons::ICONS;

pub struct Renamer {
    workspaces: Mutex<FxHashSet<i32>>,
    cfg: Mutex<Config>,
    args: Args,
}

impl Renamer {
    pub fn new(cfg: Config, args: Args) -> Self {
        Renamer {
            workspaces: Mutex::new(FxHashSet::default()),
            cfg: Mutex::new(cfg),
            args,
        }
    }

    #[inline(always)]
    pub fn renameworkspace(&self) -> Result<(), Box<dyn Error + '_>> {
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

            let title = client.title;
            if self.cfg.lock()?.config.exclude.iter().any(|(c, t)| {
                c == &class.to_uppercase() && [title.to_uppercase(), "*".to_string()].contains(t)
            }) {
                if self.args.verbose {
                    println!("- window: class '{class}' with title '{title}' is exclude")
                }
                continue;
            }

            let workspace_id = client.workspace.id;
            let icon = self.class_to_icon(&class, &title);
            let is_dup = !deduper.insert(format!("{workspace_id}-{icon}"));
            let should_dedup = self.args.dedup && is_dup;

            if self.args.verbose && should_dedup {
                println!("- window: class '{class}' is duplicate")
            } else if self.args.verbose {
                println!("- window: class '{class}', title '{title}', got this icon '{icon}'")
            };

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
            .try_for_each(|(&id, apps)| rename_cmd(id, &format!("{apps} ")))?;

        Ok(())
    }

    pub fn reset_workspaces(&self) -> Result<(), Box<dyn Error + '_>> {
        self.workspaces
            .lock()?
            .iter()
            .try_for_each(|&id| rename_cmd(id, ""))
    }

    pub fn start_listeners(self: &Arc<Self>) -> Result<(), Box<dyn Error>> {
        let mut event_listener = EventListener::new();

        rename_workspace_if!(
            self,
            event_listener,
            add_window_open_handler,
            add_window_close_handler,
            add_window_moved_handler,
            add_active_window_change_handler,
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
    pub fn watch_config_changes(&self) -> Result<(), Box<dyn Error + '_>> {
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
    fn class_to_icon(&self, class: &str, title: &str) -> String {
        let default_value = String::from("");
        let cfg = &self.cfg.lock().expect("Unable to obtain lock for config");
        cfg.config
            .icons
            .get(class)
            .or_else(|| cfg.config.icons.get(class.to_uppercase().as_str()))
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                if self.args.verbose {
                    println!("- window: class '{class}' need a shiny icon");
                }
                let icon: String = match Renamer::contains(class) {
                    Some(ic) => ic,
                    None => match Renamer::contains(title) {
                        Some(icc) => icc,
                        None => default_value,
                    },
                };
                icon
            })
    }

    fn contains(text: &str) -> Option<String> {
        ICONS.iter().find_map(|(key, val)| {
            if text.to_lowercase().contains(key) {
                Some(val.to_string())
            } else {
                None
            }
        })
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
