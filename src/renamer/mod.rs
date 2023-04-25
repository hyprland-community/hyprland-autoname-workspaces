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
        let mut counters: FxHashMap<String, i32> = FxHashMap::default();
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
            if self
                .cfg
                .lock()?
                .config
                .exclude
                .iter()
                .any(|(c, t)| c.is_match(&class) && (t.is_match(&title)))
            {
                if self.args.verbose {
                    println!("- window: class '{class}' with title '{title}' is exclude")
                }
                continue;
            }

            let workspace_id = client.workspace.id;
            let icon = self
                .class_title_to_icon(&class, &title)
                .unwrap_or_else(|| self.class_to_icon(&class, &title));

            let workspace_icon_key = format!("{workspace_id}-{icon}");

            let counter = counters
                .entry(workspace_icon_key)
                .and_modify(|count| {
                    *count += 1;
                })
                .or_insert(1);
            let counter_super = to_superscript(*counter)?;

            let should_dedup = self.args.dedup && (*counter > 1);

            let prev_counter = *counter - 1;
            let prev_counter_super = to_superscript(prev_counter)?;

            if self.args.verbose && should_dedup {
                println!("- window: class '{class}' is duplicate {counter}x")
            } else if self.args.verbose {
                println!("- window: class '{class}', title '{title}', got this icon '{icon}'")
            };

            self.workspaces.lock()?.insert(workspace_id);

            let workspace = workspaces
                .entry(workspace_id)
                .or_insert_with(|| "".to_string());

            if client.fullscreen && should_dedup && self.args.counter {
                *workspace = workspace.replace(
                    &format!("{icon}{}", *counter - 1),
                    &format!("[{icon}{counter}]"),
                );
            } else if client.fullscreen && should_dedup {
                *workspace = workspace.replace(&icon, &format!("[{icon}]"));
            } else if client.fullscreen && !should_dedup {
                *workspace = format!("{workspace} [{icon}]");
            } else if !should_dedup {
                *workspace = format!("{workspace} {icon}");
            } else if self.args.counter && should_dedup {
                if *counter > 2 {
                    *workspace = workspace.replace(
                        &format!("{icon}{}", prev_counter_super),
                        &format!("{icon}{}", counter_super),
                    );
                } else {
                    *workspace = workspace.replace(&icon, &format!("{icon}{}", counter_super));
                }
            }
        }

        workspaces
            .iter()
            .try_for_each(|(&id, apps)| rename_cmd(id, apps))?;

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
        let default_value = String::from("no default icon");
        let cfg = &self.cfg.lock().expect("Unable to obtain lock for config");
        cfg.config
            .icons
            .iter()
            .find(|(re_class, _)| re_class.is_match(class))
            .map(|(_, icon)| icon.clone())
            .unwrap_or_else(|| {
                if self.args.verbose {
                    println!("- window: class '{class}' need a shiny icon");
                }
                cfg.config
                    .icons
                    .iter()
                    .find(|(re_class, _)| re_class.to_string() == "DEFAULT")
                    .map(|(_, icon)| icon.clone())
                    .unwrap_or(default_value)
            })
            .replace("${class}", class)
            .replace("${title}", title)
    }

    #[inline(always)]
    fn class_title_to_icon(&self, class: &str, title: &str) -> Option<String> {
        let cfg = &self.cfg.lock().expect("Unable to obtain lock for config");
        cfg.config
            .title
            .iter()
            .find(|(re_class, _)| re_class.is_match(class))
            .and_then(|(_, title_icon)| {
                title_icon
                    .iter()
                    .find(|(re_title, _)| re_title.is_match(title))
                    .map(|(_, icon)| {
                        icon.to_string()
                            .replace("${class}", class)
                            .replace("${title}", title)
                    })
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

pub fn to_superscript(number: i32) -> Result<String, Box<dyn Error>> {
    let s = number.to_string();

    let mut m: FxHashMap<i32, &str> = FxHashMap::default();
    m.insert(0, "⁰");
    m.insert(1, "¹");
    m.insert(2, "²");
    m.insert(3, "³");
    m.insert(4, "⁴");
    m.insert(5, "⁵");
    m.insert(6, "⁶");
    m.insert(7, "⁷");
    m.insert(8, "⁸");
    m.insert(9, "⁹");

    let mut result: Vec<&str> = Vec::new();
    for c in s.chars() {
        let number = c.to_string().parse::<i32>()?;
        result.push(m[&number]);
    }

    Ok(result.join(""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_class_kitty() {
        let cfg_path = PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
        _ = crate::config::create_default_config(&cfg_path);
        let config = crate::config::read_config_file(&cfg_path).unwrap();
        let renamer = Renamer::new(
            Config { cfg_path, config },
            Args {
                verbose: false,
                dedup: false,
                counter: false,
            },
        );
        assert_eq!(renamer.class_to_icon("kittY", "#"), "term");
        assert_eq!(renamer.class_to_icon("Kitty", "~"), "term");
    }

    #[test]
    fn test_class_with_bad_values() {
        let cfg_path = PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
        _ = crate::config::create_default_config(&cfg_path);
        let config = crate::config::read_config_file(&cfg_path).unwrap();
        let renamer = Renamer::new(
            Config { cfg_path, config },
            Args {
                verbose: false,
                dedup: false,
                counter: false,
            },
        );
        assert_eq!(
            renamer.class_to_icon("class", "title"),
            "\u{f059} class: title"
        );
    }

    #[test]
    fn test_class_kitty_title_neomutt() {
        let cfg_path = PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
        _ = crate::config::create_default_config(&cfg_path);
        let config = crate::config::read_config_file(&cfg_path).unwrap();
        let renamer = Renamer::new(
            Config { cfg_path, config },
            Args {
                verbose: false,
                dedup: false,
                counter: false,
            },
        );
        assert_eq!(
            renamer.class_title_to_icon("kitty", "neomutt"),
            Some("neomutt".into())
        );
        assert_eq!(
            renamer.class_title_to_icon("Kitty", "Neomutt"),
            Some("neomutt".into())
        );
    }

    #[test]
    fn test_class_title_match_with_bad_values() {
        let cfg_path = PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
        _ = crate::config::create_default_config(&cfg_path);
        let config = crate::config::read_config_file(&cfg_path).unwrap();
        let renamer = Renamer::new(
            Config { cfg_path, config },
            Args {
                verbose: false,
                dedup: false,
                counter: false,
            },
        );
        assert_eq!(renamer.class_title_to_icon("aaaa", "Neomutt"), None);
        assert_eq!(renamer.class_title_to_icon("kitty", "aaaa"), None);
        assert_eq!(renamer.class_title_to_icon("kitty", "*"), None);
    }

    #[test]
    fn test_to_superscript() {
        let input = 1234567890;
        let expected = "¹²³⁴⁵⁶⁷⁸⁹⁰";
        let output = to_superscript(input).unwrap();
        assert_eq!(expected, output);
    }
}
