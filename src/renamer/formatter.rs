use crate::renamer::ConfigFile;
use crate::renamer::IconStatus::*;
use crate::{AppClient, Renamer};
use hyprland::data::FullscreenMode;
use std::collections::HashMap;
use strfmt::strfmt;

#[derive(Clone)]
pub struct AppWorkspace {
    pub id: i32,
    pub clients: Vec<AppClient>,
}

impl AppWorkspace {
    pub fn new(id: i32, clients: Vec<AppClient>) -> Self {
        AppWorkspace { id, clients }
    }
}

impl Renamer {
    pub fn generate_workspaces_string(
        &self,
        workspaces: Vec<AppWorkspace>,
        config: &ConfigFile,
    ) -> HashMap<i32, String> {
        let vars = HashMap::from([("delim".to_string(), config.format.delim.to_string())]);
        workspaces
            .iter()
            .map(|workspace| {
                let mut counted =
                    generate_counted_clients(workspace.clients.clone(), config.format.dedup);

                if config.format.dedup {
                    let mut ordered = Vec::with_capacity(counted.len());
                    let mut used = vec![false; counted.len()];

                    for client in &workspace.clients {
                        if let Some((idx, _)) = counted
                            .iter()
                            .enumerate()
                            .find(|(idx, (c, _))| !used[*idx] && c == client)
                        {
                            ordered.push(counted[idx].clone());
                            used[idx] = true;
                        }
                    }

                    ordered.extend(counted.into_iter().enumerate().filter_map(|(idx, entry)| {
                        if used[idx] {
                            None
                        } else {
                            Some(entry)
                        }
                    }));

                    counted = ordered;
                }

                let workspace_output = counted
                    .iter_mut()
                    .map(|(client, counter)| self.handle_new_client(client, *counter, config))
                    .take(
                        config
                            .format
                            .max_clients
                            .map_or(usize::MAX, |max| max as usize),
                    )
                    .collect::<Vec<String>>();

                let delimiter = formatter("{delim}", &vars);
                let joined_string = workspace_output.join(&delimiter);

                (workspace.id, joined_string)
            })
            .collect()
    }

    fn handle_new_client(&self, client: &AppClient, counter: i32, config: &ConfigFile) -> String {
        let config_format = &config.format;
        let client = client.clone();

        let is_dedup = config_format.dedup && (counter > 1);
        let is_dedup_inactive_fullscreen = config_format.dedup_inactive_fullscreen;

        let counter_sup = to_superscript(counter);
        let prev_counter = (counter - 1).to_string();
        let prev_counter_sup = to_superscript(counter - 1);
        let delim = &config_format.delim.to_string();

        let fmt_client = &config_format.client.to_string();
        let fmt_client_active = &config_format.client_active.to_string();
        let fmt_client_fullscreen = &config_format.client_fullscreen.to_string();
        let fmt_client_dup = &config_format.client_dup.to_string();
        let fmt_client_dup_fullscreen = &config_format.client_dup_fullscreen.to_string();

        let mut vars = HashMap::from([
            ("title".to_string(), client.title.clone()),
            ("class".to_string(), client.class.clone()),
            ("counter".to_string(), counter.to_string()),
            ("counter_unfocused".to_string(), prev_counter),
            ("counter_sup".to_string(), counter_sup),
            ("counter_unfocused_sup".to_string(), prev_counter_sup),
            ("delim".to_string(), delim.to_string()),
        ]);

        // get regex captures and merge them with vars
        if let Some(re_captures) = client.matched_rule.captures() {
            merge_vars(&mut vars, re_captures);
        };

        let icon = match (client.is_active, client.matched_rule.clone()) {
            (true, c @ Inactive(_)) => {
                vars.insert("default_icon".to_string(), c.icon());
                formatter(
                    &fmt_client_active.replace("{icon}", "{default_icon}"),
                    &vars,
                )
            }
            (_, c) => c.icon(),
        };

        vars.insert("icon".to_string(), icon);
        vars.insert("client".to_string(), fmt_client.to_string());
        vars.insert("client_dup".to_string(), fmt_client_dup.to_string());
        vars.insert(
            "client_fullscreen".to_string(),
            fmt_client_fullscreen.to_string(),
        );

        if self.args.debug {
            println!("client: {client:#?}\nformatter vars => {vars:#?}");
        }

        let is_grouped = client.is_fullscreen != FullscreenMode::None
            && (client.is_active || !is_dedup_inactive_fullscreen);

        match (is_grouped, is_dedup) {
            (true, true) => formatter(fmt_client_dup_fullscreen, &vars),
            (false, true) => formatter(fmt_client_dup, &vars),
            (true, false) => formatter(fmt_client_fullscreen, &vars),
            (false, false) => formatter(fmt_client, &vars),
        }
    }
}

pub fn formatter(fmt: &str, vars: &HashMap<String, String>) -> String {
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

pub fn generate_counted_clients(
    clients: Vec<AppClient>,
    need_dedup: bool,
) -> Vec<(AppClient, i32)> {
    if need_dedup {
        let mut sorted_clients = clients;
        sorted_clients.sort_by(|a, b| {
            let bf = b.is_fullscreen != FullscreenMode::None;
            let af = a.is_fullscreen != FullscreenMode::None;
            bf.cmp(&af)
        });
        sorted_clients.sort_by(|a, b| b.is_active.cmp(&a.is_active));

        sorted_clients
            .into_iter()
            .fold(vec![], |mut state, client| {
                match state.iter_mut().find(|(c, _)| c == &client) {
                    Some(c) => c.1 += 1,
                    None => state.push((client, 1)),
                }
                state
            })
    } else {
        clients.into_iter().map(|c| (c, 1)).collect()
    }
}

fn merge_vars(map1: &mut HashMap<String, String>, map2: HashMap<String, String>) {
    map1.extend(map2);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renamer::IconConfig::*;

    #[test]
    fn test_app_workspace_new() {
        let client = AppClient {
            class: String::from("Class"),
            initial_class: String::from("Class"),
            title: String::from("Title"),
            initial_title: String::from("Title"),
            is_active: false,
            is_fullscreen: FullscreenMode::Fullscreen,
            matched_rule: Inactive(Default(String::from("DefaultIcon"))),
            is_dedup_inactive_fullscreen: false,
        };

        let workspace = AppWorkspace::new(1, vec![client]);

        assert_eq!(workspace.id, 1);
        assert_eq!(workspace.clients.len(), 1);
        assert_eq!(workspace.clients[0].class, "Class");
        assert_eq!(workspace.clients[0].title, "Title");
        assert_eq!(workspace.clients[0].is_active, false);
        assert_eq!(
            workspace.clients[0].is_fullscreen,
            FullscreenMode::Fullscreen
        );
        match &workspace.clients[0].matched_rule {
            Inactive(Default(icon)) => assert_eq!(icon, "DefaultIcon"),
            _ => panic!("Unexpected IconConfig value"),
        };
    }
}
