use crate::config::Config;
use crate::params::Args;
use hyprland::data::{Client, Clients};
use hyprland::dispatch::*;
use hyprland::event_listener::EventListenerMutable as EventListener;
use hyprland::prelude::*;
use hyprland::shared::WorkspaceType;
use inotify::{Inotify, WatchMask};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::{Arc, Mutex};
use strfmt::strfmt;

#[macro_use]
mod macros;

pub struct Renamer {
    workspaces: Mutex<HashSet<i32>>,
    cfg: Mutex<Config>,
    args: Args,
}

impl Renamer {
    pub fn new(cfg: Config, args: Args) -> Arc<Self> {
        Arc::new(Renamer {
            workspaces: Mutex::new(HashSet::default()),
            cfg: Mutex::new(cfg),
            args,
        })
    }

    #[inline(always)]
    pub fn renameworkspace(&self) -> Result<(), Box<dyn Error + '_>> {
        let mut counters: HashMap<String, i32> = HashMap::default();
        let mut workspaces = self
            .workspaces
            .lock()?
            .iter()
            .map(|&c| (c, "".to_string()))
            .collect::<HashMap<_, _>>();

        // Connect to Hyprland
        let binding = Clients::get().unwrap();

        // Filter clients
        let exclude = self.cfg.lock()?.config.exclude.clone();
        let clients = binding
            .filter(|c| !c.class.is_empty())
            .filter(|c| {
                !exclude
                    .iter()
                    .any(|(class, title)| class.is_match(&c.class) && (title.is_match(&c.title)))
            })
            .collect::<Vec<Client>>();

        for clt in clients {
            let workspace_id = clt.workspace.id;
            self.workspaces.lock()?.insert(workspace_id);

            let (client_icon, client_active_icon) = self.get_client_icons(&clt.class, &clt.title);

            let counter = counters
                .entry(format!("{workspace_id}-{}", client_icon))
                .and_modify(|count| {
                    *count += 1;
                })
                .or_insert(1);

            let workspace = workspaces.entry(workspace_id).or_insert_with(String::new);

            *workspace = self
                .handle_new_client(clt, client_icon, client_active_icon, workspace, *counter)
                .expect("- not able to handle the icon");
        }

        workspaces
            .iter()
            .try_for_each(|(&id, clients)| self.rename_cmd(id, clients))?;

        Ok(())
    }

    pub fn reset_workspaces(&self) -> Result<(), Box<dyn Error + '_>> {
        self.workspaces
            .lock()?
            .iter()
            .try_for_each(|&id| self.rename_cmd(id, ""))
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
    fn class_to_icon(&self, class: &str, active: bool) -> String {
        let default_value = "no default icon".to_string();
        let cfg = &self.cfg.lock().expect("Unable to obtain lock for config");
        let icons = if active {
            &cfg.config.icons_active
        } else {
            &cfg.config.icons
        };

        icons
            .iter()
            .find(|(re_class, _)| re_class.is_match(class))
            .map(|(_, icon)| icon.to_string())
            .unwrap_or_else(|| {
                if self.args.verbose {
                    println!("- window: class '{class}' need a shiny icon");
                }
                if active {
                    cfg.config.format.client_active.to_string()
                } else {
                    icons
                        .iter()
                        .find(|(re_class, _)| re_class.to_string() == "DEFAULT")
                        .map(|(_, icon)| icon.to_string())
                        .unwrap_or(default_value)
                }
            })
    }

    #[inline(always)]
    fn class_title_to_icon(&self, class: &str, title: &str, active: bool) -> Option<String> {
        let cfg = &self.cfg.lock().expect("Unable to obtain lock for config");
        let title_icons = if active {
            &cfg.config.title_active
        } else {
            &cfg.config.title
        };

        title_icons
            .iter()
            .find(|(re_class, _)| re_class.is_match(class))
            .and_then(|(_, title_icon)| {
                title_icon
                    .iter()
                    .find(|(re_title, _)| re_title.is_match(title))
                    .map(|(_, icon)| icon.to_string())
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

    #[inline(always)]
    fn rename_cmd(&self, id: i32, clients: &str) -> Result<(), Box<dyn Error + '_>> {
        {
            let cfg = &self.cfg.lock()?.config;
            let workspace_fmt = &cfg.format.workspace;
            let workspace_empty_fmt = &cfg.format.workspace_empty;
            let id_two_digits = format!("{:02}", id);
            let vars = HashMap::from([
                ("id".to_string(), id.to_string()),
                ("id_long".to_string(), id_two_digits),
                ("delim".to_string(), cfg.format.delim.to_string()),
                ("clients".to_string(), clients.to_string()),
            ]);
            let workspace = if !clients.is_empty() {
                formatter(workspace_fmt, &vars)
            } else {
                formatter(workspace_empty_fmt, &vars)
            };
            hyprland::dispatch!(RenameWorkspace, id, Some(workspace.trim()))?;

            Ok(())
        }
    }

    #[inline(always)]
    fn handle_new_client(
        &self,
        clt: Client,
        client_icon: String,
        client_active_icon: String,
        workspace: &str,
        counter: i32,
    ) -> Result<String, Box<dyn Error + '_>> {
        let is_active = Client::get_active()
            .unwrap_or(None)
            .map(|x| x.pid)
            .unwrap_or(0)
            == clt.pid;

        let should_dedup = self.cfg.lock()?.config.format.dedup && (counter > 1);

        if self.args.verbose && should_dedup {
            println!("- window: class '{}' is duplicate {counter}x", clt.class)
        } else if self.args.verbose {
            println!(
                "- window: class '{}', title '{}', got this icon '{client_icon}'",
                clt.class, clt.title
            )
        };

        let cfg = &self.cfg.lock()?.config;

        // Formatter strings
        let counter_sup = to_superscript(counter);
        let prev_counter = (counter - 1).to_string();
        let prev_counter_sup = to_superscript(counter - 1);
        let client_dup = &cfg.format.client_dup.to_string();
        let client_dup_fullscreen = &cfg.format.client_dup_fullscreen.to_string();
        let client_active = &cfg.format.client_active.to_string();
        let client_fullscreen = &cfg.format.client_fullscreen.to_string();
        let client = &cfg.format.client.to_string();
        let delim = &cfg.format.delim.to_string();

        let mut vars = HashMap::from([
            ("title".to_string(), clt.title),
            ("class".to_string(), clt.class),
            (
                "client_fullscreen".to_string(),
                client_fullscreen.to_string(),
            ),
            ("counter".to_string(), counter.to_string()),
            ("counter_unfocused".to_string(), prev_counter),
            ("counter_sup".to_string(), counter_sup),
            ("counter_unfocused_sup".to_string(), prev_counter_sup),
            ("delim".to_string(), delim.to_string()),
        ]);

        let icon = if is_active {
            vars.insert("default_icon".to_string(), client_icon);
            formatter(
                &client_active_icon.replace("{icon}", "{default_icon}"),
                &vars,
            )
        } else {
            client_icon
        };

        vars.insert("icon".to_string(), icon);
        vars.insert("client".to_string(), formatter(client, &vars));
        vars.insert("client_active".to_string(), formatter(client_active, &vars));

        Ok(match (clt.fullscreen, should_dedup) {
            (true, true) => {
                /* fullscreen with dedup */
                if counter > 2 {
                    let from = formatter(
                        &client_dup
                            .replace("{counter_sup}", "{counter_unfocused_sup}")
                            .replace("{counter}", "{counter_unfocused}"),
                        &vars,
                    );
                    let to = formatter(client_dup_fullscreen, &vars);
                    workspace.replace(&from, &to)
                } else {
                    let from = formatter(client_dup, &vars);
                    let to = formatter(client_dup_fullscreen, &vars);
                    workspace.replace(&from, &to)
                }
            }
            (true, false) => {
                /* fullscreen with no dedup */
                format!("{workspace}{}", formatter(client_fullscreen, &vars))
            }
            (false, true) => {
                /* no fullscreen with dedup */
                if counter > 2 {
                    let from = formatter(
                        &client_dup
                            .replace("{counter_sup}", "{counter_unfocused_sup}")
                            .replace("{counter}", "{counter_unfocused}"),
                        &vars,
                    );
                    let to = formatter(client_dup, &vars);
                    workspace.replace(&from, &to)
                } else {
                    let from = formatter(client, &vars);
                    let to = formatter(client_dup, &vars);
                    workspace.replace(&from, &to)
                }
            }
            (false, false) => {
                /* no fullscreen with no dedup */
                format!("{workspace}{}", formatter(client, &vars))
            }
        })
    }

    fn get_client_icons(&self, class: &str, title: &str) -> (String, String) {
        let client_icon = self
            .class_title_to_icon(class, title, false)
            .unwrap_or_else(|| self.class_to_icon(class, false));

        let client_active_icon = self
            .class_title_to_icon(class, title, true)
            .unwrap_or_else(|| self.class_to_icon(class, true));

        (client_icon, client_active_icon)
    }
}

pub fn to_superscript(number: i32) -> String {
    let m: HashMap<_, _> = [
        ('0', "⁰"),
        ('1', "¹"),
        ('2', "²"),
        ('3', "³"),
        ('4', "⁴"),
        ('5', "⁵"),
        ('6', "⁶"),
        ('7', "⁷"),
        ('8', "⁸"),
        ('9', "⁹"),
    ]
    .into_iter()
    .collect();

    number.to_string().chars().map(|c| m[&c]).collect()
}

fn formatter(fmt: &str, vars: &HashMap<String, String>) -> String {
    let mut result = fmt.to_owned();
    let mut i = 0;
    loop {
        if !(result.contains('{') && result.contains('}')) {
            break result;
        }
        let formatted = strfmt(&result, vars).unwrap_or_else(|_| result.clone());
        if formatted == result {
            break result;
        }
        result = formatted;
        i += 1;
        if i > 3 {
            eprintln!("placeholders loop, aborting");
            break result;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use std::sync::Once;

    static INIT: Once = Once::new();

    pub fn initialize() {
        INIT.call_once(|| {
            let cfg_path = PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
            _ = crate::config::create_default_config(&cfg_path);
        });
    }

    #[test]
    fn test_class_kitty() {
        initialize();
        let cfg_path = PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
        let config = crate::config::read_config_file(&cfg_path).unwrap();
        let renamer = Renamer::new(
            Config { cfg_path, config },
            Args {
                verbose: false,
                dump: false,
            },
        );
        assert_eq!(renamer.class_to_icon("kittY", false), "term");
        assert_eq!(renamer.class_to_icon("Kitty", false), "term");
    }

    #[test]
    fn test_class_kitty_active() {
        initialize();
        let cfg_path = PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
        let config = crate::config::read_config_file(&cfg_path).unwrap();
        let renamer = Renamer::new(
            Config { cfg_path, config },
            Args {
                verbose: false,
                dump: false,
            },
        );
        assert_eq!(
            renamer.class_to_icon("Kitty", true),
            "<span foreground='red'>{icon}</span>"
        );
    }

    #[test]
    fn test_default_active() {
        initialize();
        let cfg_path = PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
        let config = crate::config::read_config_file(&cfg_path).unwrap();
        let renamer = Renamer::new(
            Config { cfg_path, config },
            Args {
                verbose: false,
                dump: false,
            },
        );
        assert_eq!(
            renamer.class_to_icon("Chromium", true),
            "<span background='orange'>{icon}</span>{delim}"
        );
    }

    #[test]
    fn test_class_with_bad_values() {
        initialize();
        let cfg_path = PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
        let config = crate::config::read_config_file(&cfg_path).unwrap();
        let renamer = Renamer::new(
            Config { cfg_path, config },
            Args {
                verbose: false,
                dump: false,
            },
        );
        assert_eq!(
            renamer.class_to_icon("class", false),
            "\u{f059} {class}: {title}"
        );
    }

    #[test]
    fn test_class_kitty_title_neomutt() {
        initialize();
        let cfg_path = PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
        let config = crate::config::read_config_file(&cfg_path).unwrap();
        let renamer = Renamer::new(
            Config { cfg_path, config },
            Args {
                verbose: false,
                dump: false,
            },
        );
        assert_eq!(
            renamer.class_title_to_icon("kitty", "neomutt", false),
            Some("neomutt".into())
        );
        assert_eq!(
            renamer.class_title_to_icon("Kitty", "Neomutt", false),
            Some("neomutt".into())
        );
    }

    #[test]
    fn test_class_title_match_with_bad_values() {
        initialize();
        let cfg_path = PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
        let config = crate::config::read_config_file(&cfg_path).unwrap();
        let renamer = Renamer::new(
            Config { cfg_path, config },
            Args {
                verbose: false,
                dump: false,
            },
        );
        assert_eq!(renamer.class_title_to_icon("aaaa", "Neomutt", false), None);
        assert_eq!(renamer.class_title_to_icon("kitty", "aaaa", false), None);
        assert_eq!(renamer.class_title_to_icon("kitty", "*", false), None);
    }

    #[test]
    fn test_to_superscript() {
        let input = 1234567890;
        let expected = "¹²³⁴⁵⁶⁷⁸⁹⁰";
        let output = to_superscript(input);
        assert_eq!(expected, output);
    }
}
