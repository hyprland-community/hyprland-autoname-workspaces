mod formatter;
mod icon;

#[macro_use]
mod macros;

use crate::config::{Config, ConfigFile, ConfigFormatRaw};
use crate::params::Args;
use formatter::*;
use hyprland::data::{Client, Clients, Workspace};
use hyprland::dispatch::*;
use hyprland::event_listener::EventListenerMutable as EventListener;
use hyprland::prelude::*;
use hyprland::shared::Address;
use hyprland::shared::WorkspaceType;
use icon::IconConfig;
use inotify::{Inotify, WatchMask};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct Renamer {
    known_workspaces: Mutex<HashSet<i32>>,
    cfg: Mutex<Config>,
    args: Args,
}

#[derive(Clone, Debug, PartialOrd, Ord, Eq)]
pub struct AppClient {
    class: String,
    title: String,
    is_active: bool,
    is_fullscreen: bool,
    is_dedup_inactive: bool,
    matched_rule: IconConfig,
}

impl PartialEq for AppClient {
    fn eq(&self, other: &Self) -> bool {
        self.matched_rule == other.matched_rule
            && self.is_active == other.is_active
            && (self.is_dedup_inactive || self.is_fullscreen == other.is_fullscreen)
    }
}

impl AppClient {
    fn new(client: Client, is_active: bool, more_dedup: bool, matched_rule: IconConfig) -> Self {
        AppClient {
            class: client.class,
            title: client.title,
            is_active,
            is_fullscreen: client.fullscreen,
            is_dedup_inactive: more_dedup,
            matched_rule,
        }
    }
}

impl Renamer {
    pub fn new(cfg: Config, args: Args) -> Arc<Self> {
        Arc::new(Renamer {
            known_workspaces: Mutex::new(HashSet::default()),
            cfg: Mutex::new(cfg),
            args,
        })
    }

    pub fn rename_workspace(&self) -> Result<(), Box<dyn Error + '_>> {
        // Config
        let config = &self.cfg.lock()?.config.clone();

        // Rename active workspace if empty
        rename_empty_workspace(config);

        // Filter clients
        let clients = get_filtered_clients(config);

        // Get the active client
        let active_client = get_active_client();

        // Get workspaces based on open clients
        let workspaces = self.get_workspaces_from_clients(clients, active_client, config)?;

        // Generate workspace strings
        let workspaces_strings = self.generate_workspaces_string(workspaces, config);

        // Render the workspaces
        workspaces_strings
            .iter()
            .for_each(|(&id, clients)| rename_cmd(id, clients, &config.format));

        Ok(())
    }

    fn get_workspaces_from_clients(
        &self,
        clients: Vec<Client>,
        active_client: String,
        config: &ConfigFile,
    ) -> Result<Vec<AppWorkspace>, Box<dyn Error + '_>> {
        let mut workspaces = self
            .known_workspaces
            .lock()?
            .iter()
            .map(|&i| (i, Vec::new()))
            .collect::<HashMap<i32, Vec<AppClient>>>();

        for client in clients {
            let workspace_id = client.workspace.id;
            self.known_workspaces.lock()?.insert(workspace_id);
            let is_active = active_client == client.address.to_string();
            workspaces
                .entry(workspace_id)
                .or_insert_with(Vec::new)
                .push(AppClient::new(
                    client.clone(),
                    is_active,
                    config.format.dedup_inactive,
                    self.parse_icon(client.class, client.title, is_active, config),
                ));
        }

        Ok(workspaces
            .iter()
            .map(|(&id, clients)| AppWorkspace::new(id, clients.to_vec()))
            .collect())
    }

    pub fn reset_workspaces(&self, config: ConfigFile) -> Result<(), Box<dyn Error + '_>> {
        self.known_workspaces
            .lock()?
            .iter()
            .for_each(|&id| rename_cmd(id, "", &config.format));

        Ok(())
    }

    pub fn start_listeners(self: &Arc<Self>) {
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
            _ = this.rename_workspace();
            _ = this.remove_workspace(wt);
        });

        _ = event_listener.start_listener();
    }

    pub fn watch_config_changes(
        &self,
        cfg_path: Option<PathBuf>,
    ) -> Result<(), Box<dyn Error + '_>> {
        match &cfg_path {
            Some(cfg_path) => {
                loop {
                    // Watch for modify events.
                    let mut notify = Inotify::init()?;

                    notify.add_watch(cfg_path, WatchMask::MODIFY)?;
                    let mut buffer = [0; 1024];
                    notify.read_events_blocking(&mut buffer)?.last();

                    println!("Reloading config !");
                    // Clojure to force quick release of lock
                    {
                        match Config::new(cfg_path.clone()) {
                            Ok(config) => self.cfg.lock()?.config = config.config,
                            Err(err) => println!("Unable to reload config: {err:?}"),
                        }
                    }

                    // Handle event
                    // Run on window events
                    _ = self.rename_workspace();
                }
            }
            None => Ok(()),
        }
    }

    fn remove_workspace(&self, wt: WorkspaceType) -> Result<bool, Box<dyn Error + '_>> {
        Ok(match wt {
            WorkspaceType::Regular(x) => self.known_workspaces.lock()?.remove(&x.parse::<i32>()?),
            WorkspaceType::Special(_) => false,
        })
    }
}

fn rename_empty_workspace(config: &ConfigFile) {
    let config_format = &config.format;

    _ = Workspace::get_active().map(|workspace| {
        if workspace.windows == 0 {
            rename_cmd(workspace.id, "", config_format);
        }
    });
}

fn rename_cmd(id: i32, clients: &str, config_format: &ConfigFormatRaw) {
    let workspace_fmt = &config_format.workspace.to_string();
    let workspace_empty_fmt = &config_format.workspace_empty.to_string();
    let id_two_digits = format!("{:02}", id);
    let mut vars = HashMap::from([
        ("id".to_string(), id.to_string()),
        ("id_long".to_string(), id_two_digits),
        ("delim".to_string(), config_format.delim.to_string()),
    ]);
    vars.insert("clients".to_string(), clients.to_string());
    let workspace = if !clients.is_empty() {
        formatter(workspace_fmt, &vars)
    } else {
        formatter(workspace_empty_fmt, &vars)
    };

    let _ = hyprland::dispatch!(RenameWorkspace, id, Some(workspace.trim()));
}

fn get_filtered_clients(config: &ConfigFile) -> Vec<Client> {
    let binding = Clients::get().unwrap();
    let config_exclude = &config.exclude;

    binding
        .filter(|client| !client.class.is_empty())
        .filter(|client| {
            !config_exclude.iter().any(|(class, title)| {
                class.is_match(&client.class) && (title.is_match(&client.title))
            })
        })
        .collect::<Vec<Client>>()
}

fn get_active_client() -> String {
    Client::get_active()
        .unwrap_or(None)
        .map(|x| x.address)
        .unwrap_or(Address::new("0"))
        .to_string()
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use super::*;
    // use std::path::PathBuf;

    // use std::sync::Once;

    // static INIT: Once = Once::new();

    // pub fn initialize() {
    //     INIT.call_once(|| {
    //         let cfg_path = PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
    //         _ = crate::config::create_default_config(&cfg_path);
    //     });
    // }

    #[test]
    fn test_app_client_partial_eq() {
        let client1 = AppClient {
            class: "kitty".to_string(),
            title: "~".to_string(),
            is_active: false,
            is_fullscreen: false,
            matched_rule: IconConfig::Class("(kitty|alacritty)".to_string(), "term".to_string()),
            is_dedup_inactive: false,
        };

        let client2 = AppClient {
            class: "alacritty".to_string(),
            title: "xplr".to_string(),
            is_active: false,
            is_fullscreen: false,
            matched_rule: IconConfig::Class("(kitty|alacritty)".to_string(), "term".to_string()),
            is_dedup_inactive: false,
        };

        let client3 = AppClient {
            class: "kitty".to_string(),
            title: "".to_string(),
            is_active: true,
            is_fullscreen: false,
            matched_rule: IconConfig::Class("(kitty|alacritty)".to_string(), "term".to_string()),
            is_dedup_inactive: false,
        };

        let client4 = AppClient {
            class: "alacritty".to_string(),
            title: "".to_string(),
            is_active: false,
            is_fullscreen: true,
            matched_rule: IconConfig::Class("(kitty|alacritty)".to_string(), "term".to_string()),
            is_dedup_inactive: false,
        };

        let client5 = AppClient {
            class: "kitty".to_string(),
            title: "".to_string(),
            is_active: false,
            is_fullscreen: true,
            matched_rule: IconConfig::Class("(kitty|alacritty)".to_string(), "term".to_string()),
            is_dedup_inactive: false,
        };

        let client6 = AppClient {
            class: "alacritty".to_string(),
            title: "".to_string(),
            is_active: false,
            is_fullscreen: false,
            matched_rule: IconConfig::Class("alacritty".to_string(), "term".to_string()),
            is_dedup_inactive: false,
        };

        assert_eq!(client1 == client2, true);
        assert_eq!(client4 == client5, true);
        assert_eq!(client1 == client4, false);
        assert_eq!(client1 == client3, false);
        assert_eq!(client5 == client6, false);
    }

    #[test]
    fn test_dedup_kitty_and_alacritty_if_one_regex() {
        let mut config = crate::config::read_config_file(None).unwrap();
        config
            .icons
            .push((Regex::new("(kitty|alacritty)").unwrap(), "term".to_string()));

        config.format.dedup_active = true;
        config.format.client_dup = "{icon}{counter}".to_string();

        let renamer = Renamer::new(
            Config {
                cfg_path: None,
                config: config.clone(),
            },
            Args {
                verbose: false,
                debug: false,
                config: None,
                dump: false,
            },
        );

        let expected = [(1, "term5".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_dedup_kitty_and_alacritty_if_two_regex() {
        let mut config = crate::config::read_config_file(None).unwrap();
        config
            .icons
            .push((Regex::new("kitty").unwrap(), "term".to_string()));

        config
            .icons
            .push((Regex::new("alacritty").unwrap(), "term".to_string()));

        config.format.dedup_active = true;
        config.format.client_dup = "{icon}{counter}".to_string();

        let renamer = Renamer::new(
            Config {
                cfg_path: None,
                config: config.clone(),
            },
            Args {
                verbose: false,
                debug: false,
                config: None,
                dump: false,
            },
        );

        let expected = [(1, "term2 term3".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_to_superscript() {
        let input = 1234567890;
        let expected = "¹²³⁴⁵⁶⁷⁸⁹⁰";
        let output = to_superscript(input);
        assert_eq!(expected, output);
    }

    #[test]
    fn test_no_dedup_no_focus_no_fullscreen_one_workspace() {
        let mut config = crate::config::read_config_file(None).unwrap();
        config
            .icons
            .push((Regex::new("kitty").unwrap(), "term".to_string()));

        let renamer = Renamer::new(
            Config {
                cfg_path: None,
                config: config.clone(),
            },
            Args {
                verbose: false,
                debug: false,
                config: None,
                dump: false,
            },
        );

        let expected = [(1, "term term term term term".to_string())]
            .into_iter()
            .collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_dedup_focus_no_fullscreen_one_workspace_middle() {
        let mut config = crate::config::read_config_file(None).unwrap();
        config
            .icons
            .push((Regex::new("kitty").unwrap(), "term".to_string()));
        config.format.client_active = "*{icon}*".to_string();

        let renamer = Renamer::new(
            Config {
                cfg_path: None,
                config: config.clone(),
            },
            Args {
                verbose: false,
                debug: false,
                dump: false,
                config: None,
            },
        );

        let expected = [(1, "term term *term* term term".to_string())]
            .into_iter()
            .collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: true,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_dedup_no_focus_fullscreen_one_workspace_middle() {
        let mut config = crate::config::read_config_file(None).unwrap();
        config
            .icons
            .push((Regex::new("kitty").unwrap(), "term".to_string()));
        config.format.client_active = "*{icon}*".to_string();
        config.format.client_fullscreen = "[{icon}]".to_string();

        let renamer = Renamer::new(
            Config {
                cfg_path: None,
                config: config.clone(),
            },
            Args {
                verbose: false,
                debug: false,
                config: None,
                dump: false,
            },
        );

        let expected = [(1, "term term [term] term term".to_string())]
            .into_iter()
            .collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: true,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_dedup_focus_fullscreen_one_workspace_middle() {
        let mut config = crate::config::read_config_file(None).unwrap();
        config
            .icons
            .push((Regex::new("kitty").unwrap(), "term".to_string()));
        config.format.client_active = "*{icon}*".to_string();
        config.format.client_fullscreen = "[{icon}]".to_string();

        let renamer = Renamer::new(
            Config {
                cfg_path: None,
                config: config.clone(),
            },
            Args {
                verbose: false,
                debug: false,
                config: None,
                dump: false,
            },
        );

        let expected = [(1, "term term [*term*] term term".to_string())]
            .into_iter()
            .collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: true,
                        is_fullscreen: true,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_dedup_no_focus_no_fullscreen_one_workspace() {
        let mut config = crate::config::read_config_file(None).unwrap();
        config
            .icons
            .push((Regex::new("kitty").unwrap(), "term".to_string()));
        config.format.dedup_active = true;
        config.format.client_dup = "{icon}{counter}".to_string();

        let renamer = Renamer::new(
            Config {
                cfg_path: None,
                config: config.clone(),
            },
            Args {
                verbose: false,
                debug: false,
                config: None,
                dump: false,
            },
        );

        let expected = [(1, "term5".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: IconConfig::Class("kitty".to_string(), "term".to_string()),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: IconConfig::Class("kitty".to_string(), "term".to_string()),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: IconConfig::Class("kitty".to_string(), "term".to_string()),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: IconConfig::Class("kitty".to_string(), "term".to_string()),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: IconConfig::Class("kitty".to_string(), "term".to_string()),
                        is_dedup_inactive: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_dedup_focus_no_fullscreen_one_workspace_middle() {
        let mut config = crate::config::read_config_file(None).unwrap();
        config
            .icons
            .push((Regex::new("kitty").unwrap(), "term".to_string()));

        config.format.dedup_active = true;
        config.format.client_dup = "{icon}{counter}".to_string();
        config.format.client_active = "*{icon}*".to_string();
        config.format.client_dup_active = "{icon}{counter_unfocused}".to_string();

        let renamer = Renamer::new(
            Config {
                cfg_path: None,
                config: config.clone(),
            },
            Args {
                verbose: false,
                debug: false,
                config: None,
                dump: false,
            },
        );

        let expected = [(1, "*term* term4".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: true,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_dedup_no_focus_fullscreen_one_workspace_middle() {
        let mut config = crate::config::read_config_file(None).unwrap();
        config
            .icons
            .push((Regex::new("kitty").unwrap(), "term".to_string()));

        config.format.dedup_active = true;
        config.format.client_dup = "{icon}{counter}".to_string();
        config.format.client_dup_fullscreen =
            "[{icon}]{delim}{icon}{counter_unfocused_sup}".to_string();

        let renamer = Renamer::new(
            Config {
                cfg_path: None,
                config: config.clone(),
            },
            Args {
                verbose: false,
                debug: false,
                config: None,
                dump: false,
            },
        );

        let expected = [(1, "[term] term4".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: true,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_dedup_focus_fullscreen_one_workspace_middle() {
        let mut config = crate::config::read_config_file(None).unwrap();
        config
            .icons
            .push((Regex::new("kitty").unwrap(), "term".to_string()));
        config.format.dedup_active = true;
        config.format.client = "{icon}".to_string();
        config.format.client_active = "*{icon}*".to_string();
        config.format.client_fullscreen = "[{icon}]".to_string();
        config.format.client_dup = "{icon}{counter}".to_string();
        config.format.client_dup_fullscreen =
            "[{icon}]{delim}{icon}{counter_unfocused}".to_string();
        config.format.client_dup_active = "*{icon}*{delim}{icon}{counter_unfocused}".to_string();

        let renamer = Renamer::new(
            Config {
                cfg_path: None,
                config: config.clone(),
            },
            Args {
                verbose: false,
                debug: false,
                config: None,
                dump: false,
            },
        );

        let expected = [(1, "[*term*] term4".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: true,
                        is_fullscreen: true,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_default_active_icon() {
        let mut config = crate::config::read_config_file(None).unwrap();
        config
            .icons
            .push((Regex::new("kitty").unwrap(), "k".to_string()));
        config
            .icons
            .push((Regex::new("alacritty").unwrap(), "a".to_string()));
        config
            .icons
            .push((Regex::new("DEFAULT").unwrap(), "d".to_string()));

        config
            .icons_active
            .push((Regex::new("kitty").unwrap(), "KKK".to_string()));
        config
            .icons_active
            .push((Regex::new("DEFAULT").unwrap(), "DDD".to_string()));

        config.format.client_active = "*{icon}*".to_string();

        let renamer = Renamer::new(
            Config {
                cfg_path: None,
                config: config.clone(),
            },
            Args {
                verbose: false,
                debug: false,
                config: None,
                dump: false,
            },
        );

        let expected = [(1, "KKK *a* DDD".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        is_active: true,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        is_active: true,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                    AppClient {
                        class: "qute".to_string(),
                        title: "qute".to_string(),
                        is_active: true,
                        is_fullscreen: false,
                        matched_rule: renamer.parse_icon(
                            "qute".to_string(),
                            "qute".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_default_class_active_fallback_to_class_default() {
        let mut config = crate::config::read_config_file(None).unwrap();

        config
            .icons_active
            .push((Regex::new("DEFAULT").unwrap(), "default active".to_string()));

        println!("config => {:#?}", config);
        let renamer = Renamer::new(
            Config {
                cfg_path: None,
                config: config.clone(),
            },
            Args {
                verbose: false,
                debug: false,
                config: None,
                dump: false,
            },
        );

        let expected = [(1, "default active".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![AppClient {
                    class: "kitty".to_string(),
                    title: "~".to_string(),
                    is_active: true,
                    is_fullscreen: false,
                    matched_rule: renamer.parse_icon(
                        "kitty".to_string(),
                        "~".to_string(),
                        true,
                        &config,
                    ),
                    is_dedup_inactive: false,
                }],
            }],
            &config,
        );

        assert_eq!(actual, expected);

        let config = crate::config::read_config_file(None).unwrap();

        let renamer = Renamer::new(
            Config {
                cfg_path: None,
                config: config.clone(),
            },
            Args {
                verbose: false,
                debug: false,
                config: None,
                dump: false,
            },
        );

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![AppClient {
                    class: "kitty".to_string(),
                    title: "~".to_string(),
                    is_active: true,
                    is_fullscreen: false,
                    matched_rule: renamer.parse_icon(
                        "kitty".to_string(),
                        "~".to_string(),
                        true,
                        &config,
                    ),
                    is_dedup_inactive: false,
                }],
            }],
            &config,
        );

        let expected = [(1, "\u{f059} kitty".to_string())].into_iter().collect();

        assert_eq!(actual, expected);
    }
}
