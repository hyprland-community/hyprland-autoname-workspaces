mod formatter;
mod icon;

#[macro_use]
mod macros;

use crate::config::{Config, ConfigFile, ConfigFormatRaw};
use crate::params::Args;
use formatter::*;
use hyprland::data::{Client, Clients, FullscreenMode, Workspace};
use hyprland::dispatch::*;
use hyprland::event_listener::{EventListener, WorkspaceDestroyedEventData};
use hyprland::prelude::*;
use hyprland::shared::Address;
use icon::{IconConfig, IconStatus};
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

#[derive(Clone, Eq, Debug)]
pub struct AppClient {
    class: String,
    title: String,
    //FIXME: I can't understand why clippy
    // see dead code, but for me, my code is not dead!
    #[allow(dead_code)]
    initial_class: String,
    #[allow(dead_code)]
    initial_title: String,
    is_active: bool,
    is_fullscreen: FullscreenMode,
    is_dedup_inactive_fullscreen: bool,
    matched_rule: IconStatus,
}

impl PartialEq for AppClient {
    fn eq(&self, other: &Self) -> bool {
        self.matched_rule == other.matched_rule
            && self.is_active == other.is_active
            && (self.is_dedup_inactive_fullscreen || self.is_fullscreen == other.is_fullscreen)
    }
}

impl AppClient {
    fn new(
        client: Client,
        is_active: bool,
        is_dedup_inactive_fullscreen: bool,
        matched_rule: IconStatus,
    ) -> Self {
        AppClient {
            initial_class: client.initial_class,
            class: client.class,
            initial_title: client.initial_title,
            title: client.title,
            is_active,
            is_fullscreen: client.fullscreen,
            is_dedup_inactive_fullscreen,
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
        workspaces_strings.iter().for_each(|(&id, clients)| {
            rename_cmd(id, clients, &config.format, &config.workspaces_name)
        });

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

        let is_dedup_inactive_fullscreen = config.format.dedup_inactive_fullscreen;

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
                    is_dedup_inactive_fullscreen,
                    self.parse_icon(
                        client.initial_class,
                        client.class,
                        client.initial_title,
                        client.title,
                        is_active,
                        config,
                    ),
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
            .for_each(|&id| rename_cmd(id, "", &config.format, &config.workspaces_name));

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
            add_fullscreen_state_change_handler,
            add_window_title_change_handler
        );

        let this = self.clone();
        event_listener.add_workspace_destroy_handler(move |wt| {
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

                    notify.watches().add(cfg_path, WatchMask::MODIFY)?;
                    let mut buffer = [0; 1024];
                    notify.read_events_blocking(&mut buffer)?.last();

                    println!("Reloading config !");
                    // Clojure to force quick release of lock
                    {
                        match Config::new(cfg_path.clone(), false, false) {
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

    fn remove_workspace(
        &self,
        wt: WorkspaceDestroyedEventData,
    ) -> Result<bool, Box<dyn Error + '_>> {
        Ok(self.known_workspaces.lock()?.remove(&wt.workspace_id))
    }
}

fn rename_empty_workspace(config: &ConfigFile) {
    _ = Workspace::get_active().map(|workspace| {
        if workspace.windows == 0 {
            rename_cmd(workspace.id, "", &config.format, &config.workspaces_name);
        }
    });
}

fn rename_cmd(
    id: i32,
    clients: &str,
    config_format: &ConfigFormatRaw,
    workspaces_name: &[(String, String)],
) {
    let workspace_fmt = &config_format.workspace.to_string();
    let workspace_empty_fmt = &config_format.workspace_empty.to_string();
    let id_two_digits = format!("{:02}", id);
    let workspace_name = get_workspace_name(id, workspaces_name);

    let mut vars = HashMap::from([
        ("id".to_string(), id.to_string()),
        ("id_long".to_string(), id_two_digits),
        ("name".to_string(), workspace_name),
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

fn get_workspace_name(id: i32, workspaces_name: &[(String, String)]) -> String {
    let default_workspace_name = id.to_string();
    workspaces_name
        .iter()
        .find_map(|(x, name)| {
            if x.eq(&id.to_string()) {
                Some(name)
            } else {
                None
            }
        })
        .unwrap_or(&default_workspace_name)
        .to_string()
}

fn get_filtered_clients(config: &ConfigFile) -> Vec<Client> {
    let binding = Clients::get().unwrap();
    let config_exclude = &config.exclude;

    binding
        .into_iter()
        .filter(|client| client.pid > 0)
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
    use crate::renamer::IconConfig::*;
    use crate::renamer::IconStatus::*;

    #[test]
    fn test_app_client_partial_eq() {
        let client1 = AppClient {
            initial_class: "kitty".to_string(),
            class: "kitty".to_string(),
            title: "~".to_string(),
            is_active: false,
            is_fullscreen: FullscreenMode::Fullscreen,
            initial_title: "zsh".to_string(),
            matched_rule: Inactive(Class("(kitty|alacritty)".to_string(), "term".to_string())),
            is_dedup_inactive_fullscreen: false,
        };

        let client2 = AppClient {
            initial_class: "alacritty".to_string(),
            class: "alacritty".to_string(),
            title: "xplr".to_string(),
            initial_title: "zsh".to_string(),
            is_active: false,
            is_fullscreen: FullscreenMode::Fullscreen,
            matched_rule: Inactive(Class("(kitty|alacritty)".to_string(), "term".to_string())),
            is_dedup_inactive_fullscreen: false,
        };

        let client3 = AppClient {
            initial_class: "kitty".to_string(),
            class: "kitty".to_string(),
            title: "".to_string(),
            initial_title: "zsh".to_string(),
            is_active: true,
            is_fullscreen: FullscreenMode::None,
            matched_rule: Active(Class("(kitty|alacritty)".to_string(), "term".to_string())),
            is_dedup_inactive_fullscreen: false,
        };

        let client4 = AppClient {
            initial_class: "alacritty".to_string(),
            class: "alacritty".to_string(),
            title: "".to_string(),
            initial_title: "zsh".to_string(),
            is_active: false,
            is_fullscreen: FullscreenMode::Fullscreen,
            matched_rule: Inactive(Class("(kitty|alacritty)".to_string(), "term".to_string())),
            is_dedup_inactive_fullscreen: false,
        };

        let client5 = AppClient {
            initial_class: "kitty".to_string(),
            class: "kitty".to_string(),
            title: "".to_string(),
            initial_title: "zsh".to_string(),
            is_active: false,
            is_fullscreen: FullscreenMode::Fullscreen,
            matched_rule: Inactive(Class("(kitty|alacritty)".to_string(), "term".to_string())),
            is_dedup_inactive_fullscreen: false,
        };

        let client6 = AppClient {
            initial_class: "alacritty".to_string(),
            class: "alacritty".to_string(),
            title: "".to_string(),
            initial_title: "zsh".to_string(),
            is_active: false,
            is_fullscreen: FullscreenMode::None,
            matched_rule: Inactive(Class("alacritty".to_string(), "term".to_string())),
            is_dedup_inactive_fullscreen: false,
        };

        assert_eq!(client1 == client2, true);
        assert_eq!(client4 == client5, true);
        assert_eq!(client1 == client4, true);
        assert_eq!(client1 == client3, false);
        assert_eq!(client5 == client6, false);
    }

    #[test]
    fn test_dedup_kitty_and_alacritty_if_one_regex() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();
        config
            .class
            .push((Regex::new("(kitty|alacritty)").unwrap(), "term".to_string()));

        config.format.dedup = true;
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
                migrate_config: false,
            },
        );

        let expected = [(1, "term5".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "alacritty".to_string(),
                        class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        initial_title: "alacritty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "alacritty".to_string(),
                        initial_class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        initial_title: "alacritty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "alacritty".to_string(),
                        class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        initial_title: "alacritty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_icon_initial_title_and_initial_title_active() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();
        config
            .class
            .push((Regex::new("kitty").unwrap(), "term".to_string()));

        config
            .class
            .push((Regex::new("alacritty").unwrap(), "term".to_string()));

        config.initial_title_in_class.push((
            Regex::new("(kitty|alacritty)").unwrap(),
            vec![(Regex::new("zsh").unwrap(), "Zsh".to_string())],
        ));

        config.initial_title_in_class_active.push((
            Regex::new("alacritty").unwrap(),
            vec![(Regex::new("zsh").unwrap(), "#Zsh#".to_string())],
        ));

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
                migrate_config: false,
            },
        );

        let expected = [(1, "Zsh #Zsh# *Zsh*".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        initial_class: "alacritty".to_string(),
                        class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        initial_title: "zsh".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "zsh".to_string(),
                            "alacritty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "alacritty".to_string(),
                        class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        initial_title: "zsh".to_string(),
                        is_active: true,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "zsh".to_string(),
                            "alacritty".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "~".to_string(),
                        initial_title: "zsh".to_string(),
                        is_active: true,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "zsh".to_string(),
                            "~".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                ],
            }],
            &config,
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_dedup_kitty_and_alacritty_if_two_regex() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();
        config
            .class
            .push((Regex::new("kitty").unwrap(), "term".to_string()));

        config
            .class
            .push((Regex::new("alacritty").unwrap(), "term".to_string()));

        config.format.dedup = true;
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
                migrate_config: false,
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
                        initial_class: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "alacritty".to_string(),
                        initial_class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        initial_title: "alacritty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "alacritty".to_string(),
                        initial_class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        initial_title: "alacritty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "alacritty".to_string(),
                        class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        initial_title: "alacritty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
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
        let mut config = crate::config::read_config_file(None, false, false).unwrap();
        config
            .class
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
                migrate_config: false,
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
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_dedup_focus_no_fullscreen_one_workspace_middle() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();
        config
            .class
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
                migrate_config: false,
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
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: true,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_dedup_no_focus_fullscreen_one_workspace_middle() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();
        config
            .class
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
                dump: false,
                migrate_config: false,
                config: None,
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
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::Fullscreen,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_dedup_focus_fullscreen_one_workspace_middle() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();
        config
            .class
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
                dump: false,
                migrate_config: false,
                config: None,
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
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: true,
                        is_fullscreen: FullscreenMode::Fullscreen,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_dedup_no_focus_no_fullscreen_one_workspace() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();
        config
            .class
            .push((Regex::new("kitty").unwrap(), "term".to_string()));
        config.format.dedup = true;
        config.format.client_dup = "{icon}{counter}".to_string();

        let renamer = Renamer::new(
            Config {
                cfg_path: None,
                config: config.clone(),
            },
            Args {
                verbose: false,
                debug: false,
                dump: false,
                migrate_config: false,
                config: None,
            },
        );

        let expected = [(1, "term5".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: Inactive(Class("kitty".to_string(), "term".to_string())),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: Inactive(Class("kitty".to_string(), "term".to_string())),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: Inactive(Class("kitty".to_string(), "term".to_string())),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: Inactive(Class("kitty".to_string(), "term".to_string())),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: Inactive(Class("kitty".to_string(), "term".to_string())),
                        is_dedup_inactive_fullscreen: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_dedup_focus_no_fullscreen_one_workspace_middle() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();
        config
            .class
            .push((Regex::new("kitty").unwrap(), "term".to_string()));

        config.format.dedup = true;
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
                dump: false,
                migrate_config: false,
                config: None,
            },
        );

        let expected = [(1, "*term* term4".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: true,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_dedup_no_focus_fullscreen_one_workspace_middle() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();
        config
            .class
            .push((Regex::new("kitty").unwrap(), "term".to_string()));

        config.format.dedup = true;
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
                migrate_config: false,
            },
        );

        let expected = [(1, "[term] term4".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::Fullscreen,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_dedup_focus_fullscreen_one_workspace_middle() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();
        config
            .class
            .push((Regex::new("kitty").unwrap(), "term".to_string()));
        config.format.dedup = true;
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
                migrate_config: false,
            },
        );

        let expected = [(1, "[*term*] term4".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: true,
                        is_fullscreen: FullscreenMode::Fullscreen,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "kitty".to_string(),
                        initial_class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: false,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            false,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_default_active_icon() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();
        config
            .class
            .push((Regex::new("kitty").unwrap(), "k".to_string()));
        config
            .class
            .push((Regex::new("alacritty").unwrap(), "a".to_string()));
        config
            .class
            .push((Regex::new("DEFAULT").unwrap(), "d".to_string()));

        config
            .class_active
            .push((Regex::new("kitty").unwrap(), "KKK".to_string()));
        config
            .class_active
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
                migrate_config: false,
            },
        );

        let expected = [(1, "KKK *a* DDD".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![
                    AppClient {
                        initial_class: "kitty".to_string(),
                        class: "kitty".to_string(),
                        title: "kitty".to_string(),
                        initial_title: "kitty".to_string(),
                        is_active: true,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            "kitty".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "alacritty".to_string(),
                        initial_class: "alacritty".to_string(),
                        title: "alacritty".to_string(),
                        initial_title: "alacritty".to_string(),
                        is_active: true,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            "alacritty".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                    AppClient {
                        class: "qute".to_string(),
                        initial_class: "qute".to_string(),
                        title: "qute".to_string(),
                        initial_title: "qute".to_string(),
                        is_active: true,
                        is_fullscreen: FullscreenMode::None,
                        matched_rule: renamer.parse_icon(
                            "qute".to_string(),
                            "qute".to_string(),
                            "qute".to_string(),
                            "qute".to_string(),
                            true,
                            &config,
                        ),
                        is_dedup_inactive_fullscreen: false,
                    },
                ],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_class_but_title_icon() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();
        config.title_in_class.push((
            Regex::new("^$").unwrap(),
            vec![(Regex::new("(?i)spotify").unwrap(), "spotify".to_string())],
        ));

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
                migrate_config: false,
            },
        );

        let expected = [(1, "spotify".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![AppClient {
                    initial_class: "".to_string(),
                    class: "".to_string(),
                    title: "spotify".to_string(),
                    initial_title: "spotify".to_string(),
                    is_active: false,
                    is_fullscreen: FullscreenMode::None,
                    matched_rule: renamer.parse_icon(
                        "".to_string(),
                        "".to_string(),
                        "spotify".to_string(),
                        "spotify".to_string(),
                        false,
                        &config,
                    ),
                    is_dedup_inactive_fullscreen: false,
                }],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_class_with_exclam_mark() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();

        config
            .class
            .push((Regex::new("osu!").unwrap(), "osu".to_string()));

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
                migrate_config: false,
            },
        );

        let expected = [(1, "osu".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![AppClient {
                    initial_class: "osu!".to_string(),
                    class: "osu!".to_string(),
                    title: "osu!".to_string(),
                    initial_title: "osu!".to_string(),
                    is_active: false,
                    is_fullscreen: FullscreenMode::None,
                    matched_rule: renamer.parse_icon(
                        "osu!".to_string(),
                        "osu!".to_string(),
                        "osu!".to_string(),
                        "osu!".to_string(),
                        false,
                        &config,
                    ),
                    is_dedup_inactive_fullscreen: false,
                }],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_default_class_active_fallback_to_class_default() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();

        config
            .class_active
            .push((Regex::new("DEFAULT").unwrap(), "default active".to_string()));

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
                migrate_config: false,
            },
        );

        let expected = [(1, "default active".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![AppClient {
                    initial_class: "kitty".to_string(),
                    class: "kitty".to_string(),
                    title: "~".to_string(),
                    initial_title: "zsh".to_string(),
                    is_active: true,
                    is_fullscreen: FullscreenMode::None,
                    matched_rule: renamer.parse_icon(
                        "kitty".to_string(),
                        "kitty".to_string(),
                        "zsh".to_string(),
                        "~".to_string(),
                        true,
                        &config,
                    ),
                    is_dedup_inactive_fullscreen: false,
                }],
            }],
            &config,
        );

        assert_eq!(actual, expected);

        let config = crate::config::read_config_file(None, false, false).unwrap();

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
                migrate_config: false,
            },
        );

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![AppClient {
                    initial_class: "kitty".to_string(),
                    class: "kitty".to_string(),
                    initial_title: "zsh".to_string(),
                    title: "~".to_string(),
                    is_active: true,
                    is_fullscreen: FullscreenMode::None,
                    matched_rule: renamer.parse_icon(
                        "kitty".to_string(),
                        "kitty".to_string(),
                        "zsh".to_string(),
                        "~".to_string(),
                        true,
                        &config,
                    ),
                    is_dedup_inactive_fullscreen: false,
                }],
            }],
            &config,
        );

        let expected = [(1, "\u{f059} kitty".to_string())].into_iter().collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_initial_title_in_initial_class_combos() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();

        config
            .class
            .push((Regex::new("kitty").unwrap(), "term0".to_string()));

        config.title_in_class.push((
            Regex::new("kitty").unwrap(),
            vec![(Regex::new("~").unwrap(), "term1".to_string())],
        ));

        config.title_in_initial_class.push((
            Regex::new("kitty").unwrap(),
            vec![(Regex::new("~").unwrap(), "term2".to_string())],
        ));

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
                migrate_config: false,
            },
        );

        let expected = [(1, "term2".to_string())].into_iter().collect();

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![AppClient {
                    initial_class: "kitty".to_string(),
                    class: "kitty".to_string(),
                    title: "~".to_string(),
                    initial_title: "zsh".to_string(),
                    is_active: false,
                    is_fullscreen: FullscreenMode::None,
                    is_dedup_inactive_fullscreen: false,
                    matched_rule: renamer.parse_icon(
                        "kitty".to_string(),
                        "kitty".to_string(),
                        "zsh".to_string(),
                        "~".to_string(),
                        false,
                        &config,
                    ),
                }],
            }],
            &config,
        );

        assert_eq!(actual, expected);

        config.initial_title_in_class.push((
            Regex::new("kitty").unwrap(),
            vec![(Regex::new("(?i)zsh").unwrap(), "term3".to_string())],
        ));

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
                migrate_config: false,
            },
        );

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![AppClient {
                    initial_class: "kitty".to_string(),
                    class: "kitty".to_string(),
                    initial_title: "zsh".to_string(),
                    title: "~".to_string(),
                    is_active: false,
                    is_fullscreen: FullscreenMode::None,
                    matched_rule: renamer.parse_icon(
                        "kitty".to_string(),
                        "kitty".to_string(),
                        "zsh".to_string(),
                        "~".to_string(),
                        false,
                        &config,
                    ),
                    is_dedup_inactive_fullscreen: false,
                }],
            }],
            &config,
        );

        let expected = [(1, "term3".to_string())].into_iter().collect();

        assert_eq!(actual, expected);

        config.initial_title_in_initial_class.push((
            Regex::new("kitty").unwrap(),
            vec![(Regex::new("(?i)zsh").unwrap(), "term4".to_string())],
        ));

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
                migrate_config: false,
            },
        );

        let actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![AppClient {
                    initial_class: "kitty".to_string(),
                    class: "kitty".to_string(),
                    initial_title: "zsh".to_string(),
                    title: "~".to_string(),
                    is_active: false,
                    is_fullscreen: FullscreenMode::None,
                    matched_rule: renamer.parse_icon(
                        "kitty".to_string(),
                        "kitty".to_string(),
                        "zsh".to_string(),
                        "~".to_string(),
                        false,
                        &config,
                    ),
                    is_dedup_inactive_fullscreen: false,
                }],
            }],
            &config,
        );

        let expected = [(1, "term4".to_string())].into_iter().collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_regex_capture_support() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();

        config.title_in_class.push((
            Regex::new("(?i)foot").unwrap(),
            vec![(
                Regex::new("emerge: (.+?/.+?)-.*").unwrap(),
                "test {match1}".to_string(),
            )],
        ));
        config.title_in_class.push((
            Regex::new("(?i)foot").unwrap(),
            vec![(
                Regex::new("pacman: (.+?/.+?)-(.*)").unwrap(),
                "test {match1} test2 {match2}".to_string(),
            )],
        ));
        config.title_in_class_active.push((
            Regex::new("(?i)foot").unwrap(),
            vec![(
                Regex::new("pacman: (.+?/.+?)-(.*)").unwrap(),
                "*#test{match1}#between#{match2}endtest#*".to_string(),
            )],
        ));

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
                migrate_config: false,
            },
        );

        let mut expected = [(1, "test (13 of 20) dev-lang/rust".to_string())]
            .into_iter()
            .collect();

        let mut actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![AppClient {
                    initial_class: "foot".to_string(),
                    class: "foot".to_string(),
                    initial_title: "zsh".to_string(),
                    title: "emerge: (13 of 20) dev-lang/rust-1.69.0-r1 Compile:".to_string(),
                    is_active: false,
                    is_fullscreen: FullscreenMode::None,
                    matched_rule: renamer.parse_icon(
                        "foot".to_string(),
                        "foot".to_string(),
                        "zsh".to_string(),
                        "emerge: (13 of 20) dev-lang/rust-1.69.0-r1 Compile:".to_string(),
                        false,
                        &config,
                    ),
                    is_dedup_inactive_fullscreen: false,
                }],
            }],
            &config,
        );

        assert_eq!(actual, expected);

        expected = [(
            1,
            "*#test(14 of 20) dev-lang/rust#between#1.69.0-r1 Compile:endtest#*".to_string(),
        )]
        .into_iter()
        .collect();

        actual = renamer.generate_workspaces_string(
            vec![AppWorkspace {
                id: 1,
                clients: vec![AppClient {
                    initial_class: "foot".to_string(),
                    class: "foot".to_string(),
                    initial_title: "zsh".to_string(),
                    title: "pacman: (14 of 20) dev-lang/rust-1.69.0-r1 Compile:".to_string(),
                    is_active: true,
                    is_fullscreen: FullscreenMode::None,
                    matched_rule: renamer.parse_icon(
                        "foot".to_string(),
                        "foot".to_string(),
                        "zsh".to_string(),
                        "pacman: (14 of 20) dev-lang/rust-1.69.0-r1 Compile:".to_string(),
                        true,
                        &config,
                    ),
                    is_dedup_inactive_fullscreen: false,
                }],
            }],
            &config,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_workspaces_name_config() {
        let mut config = crate::config::read_config_file(None, false, false).unwrap();

        config
            .workspaces_name
            .push(("0".to_string(), "zero".to_string()));

        config
            .workspaces_name
            .push(("1".to_string(), "one".to_string()));

        let expected = "zero".to_string();
        let actual = get_workspace_name(0, &config.workspaces_name);

        assert_eq!(actual, expected);

        let expected = "one".to_string();
        let actual = get_workspace_name(1, &config.workspaces_name);

        assert_eq!(actual, expected);

        let expected = "3".to_string();
        let actual = get_workspace_name(3, &config.workspaces_name);

        assert_eq!(actual, expected);
    }
}
