use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

#[derive(Default, Debug)]
pub struct Config {
    pub config: ConfigFile,
    pub cfg_path: PathBuf,
}

fn default_delim_formatter() -> String {
    " ".to_string()
}

fn default_client_formatter() -> String {
    "{icon}{delim}".to_string()
}

fn default_client_active_formatter() -> String {
    "*{icon}*".to_string()
}

fn default_client_dup_formatter() -> String {
    "{icon}{counter_sup}{delim}".to_string()
}

fn default_client_fullscreen_formatter() -> String {
    "[{icon}]".to_string()
}

fn default_client_dup_fullscreen_formatter() -> String {
    "[{icon}]{delim}{icon}{counter_unfocused_sup}".to_string()
}

fn default_workspace_formatter() -> String {
    "{id}: {clients}".to_string()
}

fn default_icons() -> HashMap<String, String> {
    HashMap::from([("DEFAULT".to_string(), " {class}".to_string())])
}

#[derive(Deserialize, Default, Debug)]
pub struct ConfigFormatRaw {
    #[serde(default)]
    pub dedup: bool,
    #[serde(default = "default_delim_formatter")]
    pub delim: String,
    #[serde(default = "default_workspace_formatter")]
    pub workspace: String,
    #[serde(default = "default_client_formatter")]
    pub client: String,
    #[serde(default = "default_client_fullscreen_formatter")]
    pub client_fullscreen: String,
    #[serde(default = "default_client_active_formatter")]
    pub client_active: String,
    #[serde(default = "default_client_dup_formatter")]
    pub client_dup: String,
    #[serde(default = "default_client_dup_fullscreen_formatter")]
    pub client_dup_fullscreen: String,
}

#[derive(Deserialize, Default)]
pub struct ConfigFileRaw {
    #[serde(default = "default_icons")]
    pub icons: HashMap<String, String>,
    #[serde(default, alias = "active_icons")]
    pub icons_active: HashMap<String, String>,
    #[serde(default, alias = "title_icons", alias = "icons_title")]
    pub title: HashMap<String, HashMap<String, String>>,
    #[serde(default, alias = "title_active_icons")]
    pub title_active: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub exclude: HashMap<String, String>,
    #[serde(default)]
    pub format: ConfigFormatRaw,
}

#[derive(Default, Debug)]
pub struct ConfigFile {
    pub icons: Vec<(Regex, String)>,
    pub icons_active: Vec<(Regex, String)>,
    pub title: Vec<(Regex, Vec<(Regex, String)>)>,
    pub title_active: Vec<(Regex, Vec<(Regex, String)>)>,
    pub exclude: Vec<(Regex, Regex)>,
    pub format: ConfigFormatRaw,
}

impl Config {
    pub fn new() -> Result<Config, Box<dyn Error>> {
        let xdg_dirs = xdg::BaseDirectories::with_prefix("hyprland-autoname-workspaces")?;
        let cfg_path = xdg_dirs.place_config_file("config.toml")?;

        if !cfg_path.exists() {
            _ = create_default_config(&cfg_path);
        }

        let config = read_config_file(&cfg_path)?;

        Ok(Config { config, cfg_path })
    }
}

pub fn read_config_file(cfg_path: &PathBuf) -> Result<ConfigFile, Box<dyn Error>> {
    let config_string = fs::read_to_string(cfg_path)?;

    let config: ConfigFileRaw =
        toml::from_str(&config_string).map_err(|e| format!("Unable to parse: {e:?}"))?;

    Ok(ConfigFile {
        icons: generate_icon_config(config.icons),
        icons_active: generate_icon_config(config.icons_active),
        title: generate_title_config(config.title),
        title_active: generate_title_config(config.title_active),
        exclude: generate_exclude_config(config.exclude),
        format: config.format,
    })
}

pub fn create_default_config(cfg_path: &PathBuf) -> Result<&'static str, Box<dyn Error + 'static>> {
    // TODO: maybe we should dump the config from the default values of the struct?
    let default_config = r#"
[format]
# Deduplicate icons if enable.
# A superscripted counter will be added.
dedup = false
# window delimiter
delim = " "

# available formatter:
# {counter_sup} - superscripted count of clients on the workspace, and simple {counter}, {delim}
# {icon}, {client}
# workspace formatter
workspace = "{id}: {clients}" # {id} and {clients} supported
# client formatter
client = "{icon}{delim}"
client_active = "<span background='orange'>{icon}</span>{delim}"
# deduplicate client formatter
client_dup = "{client}{counter_sup}{delim}"
client_dup_fullscreen = "[{icon}]{delim}{icon}{counter_unfocused}{delim}"

[icons]
# Add your icons mapping
# use double quote the key and the value
# take class name from 'hyprctl clients'
"DEFAULT" = " {class}: {title}"
"(?i)Kitty" = "term"
"[Ff]irefox" = "browser"
"(?i)waydroid.*" = "droid"

[icons_active]
# DEFAULT = "{icon}" # what to do with this ?
"(?i)Kitty" = "<span foreground='red'>{icon}</span>"

[title."(?i)kitty"]
"(?i)neomutt" = "neomutt"

[title_active."(?i)firefox"]
"(?i)twitch" = "<span color='purple'>{icon}</span>"

# Add your applications that need to be exclude
# The key is the class, the value is the title.
# You can put an empty title to exclude based on
# class name only, "" make the job.
[exclude]
"(?i)fcitx" = ".*" # will match all title for fcitx
"(?i)TestApp" = "" # will match all title for TestApp
aProgram = "^$" # will match null title for aProgram
"[Ss]team" = "Friends List.*"
"[Ss]team" = "^$" # will match all Steam window with null title (some popup)
"#
    .trim();

    let mut config_file = File::create(cfg_path)?;
    write!(&mut config_file, "{default_config}")?;
    println!("Default config created in {cfg_path:?}");

    Ok(default_config)
}

fn regex_with_error_logging(pattern: &str) -> Option<Regex> {
    match Regex::new(pattern) {
        Ok(re) => Some(re),
        Err(e) => {
            println!("Unable to parse regex: {e:?}");
            None
        }
    }
}

/// Generates the title configuration for the application.
///
/// This function accepts a nested HashMap where the outer HashMap's keys represent class names,
/// and the inner HashMap's keys represent titles, and their values are icons.
/// It returns a Vec of tuples, where the first element is a Regex object created from the class name,
/// and the second element is a Vec of tuples containing a Regex object created from the title and the corresponding icon as a String.
///
/// # Arguments
///
/// * `icons` - A nested HashMap where the outer keys are class names, and the inner keys are titles with their corresponding icon values.
///
/// # Examples
///
/// ```
/// let title_icons = generate_title_config(title_icons_map);
/// ```
fn generate_title_config(
    icons: HashMap<String, HashMap<String, String>>,
) -> Vec<(Regex, Vec<(Regex, String)>)> {
    icons
        .iter()
        .filter_map(|(class, title_icon)| {
            regex_with_error_logging(class).map(|re| {
                (
                    re,
                    title_icon
                        .iter()
                        .filter_map(|(title, icon)| {
                            regex_with_error_logging(title).map(|re| (re, icon.to_string()))
                        })
                        .collect(),
                )
            })
        })
        .collect()
}

/// Generates the icon configuration for the application.
///
/// This function accepts a HashMap where the keys represent class names and the values are icons.
/// It returns a Vec of tuples, where the first element is a Regex object created from the class name,
/// and the second element is the corresponding icon as a String.
///
/// # Arguments
///
/// * `icons` - A HashMap with keys as class names and values as icons.
///
/// # Examples
///
/// ```
/// let icons_config = generate_icon_config(icons_map);
/// ```
fn generate_icon_config(icons: HashMap<String, String>) -> Vec<(Regex, String)> {
    icons
        .iter()
        .filter_map(|(class, icon)| {
            regex_with_error_logging(class).map(|re| (re, icon.to_string()))
        })
        .collect()
}

/// Generates the exclude configuration for the application.
///
/// This function accepts a HashMap where the keys represent class names and the values are titles.
/// It returns a Vec of tuples, where the first element is a Regex object created from the class name,
/// and the second element is a Regex object created from the title.
///
/// # Arguments
///
/// * `icons` - A HashMap with keys as class names and values as titles.
///
/// # Examples
///
/// ```
/// let exclude_config = generate_exclude_config(exclude_map);
/// ```
fn generate_exclude_config(icons: HashMap<String, String>) -> Vec<(Regex, Regex)> {
    icons
        .iter()
        .filter_map(|(class, title)| {
            regex_with_error_logging(class).and_then(|re_class| {
                regex_with_error_logging(title).map(|re_title| (re_class, re_title))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_generate_title_config() {
        let mut title_icons_map: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut inner_map: HashMap<String, String> = HashMap::new();
        inner_map.insert("Title1".to_string(), "Icon1".to_string());
        title_icons_map.insert("Class1".to_string(), inner_map);

        let title_config = generate_title_config(title_icons_map);

        assert_eq!(title_config.len(), 1);
        assert!(title_config[0].0.is_match("Class1"));
        assert_eq!(title_config[0].1.len(), 1);
        assert!(title_config[0].1[0].0.is_match("Title1"));
        assert_eq!(title_config[0].1[0].1, "Icon1");
    }

    #[test]
    fn test_generate_icon_config() {
        let mut icons_map: HashMap<String, String> = HashMap::new();
        icons_map.insert("Class1".to_string(), "Icon1".to_string());

        let icons_config = generate_icon_config(icons_map);

        assert_eq!(icons_config.len(), 1);
        assert!(icons_config[0].0.is_match("Class1"));
        assert_eq!(icons_config[0].1, "Icon1");
    }

    #[test]
    fn test_generate_exclude_config() {
        let mut exclude_map: HashMap<String, String> = HashMap::new();
        exclude_map.insert("Class1".to_string(), "Title1".to_string());

        let exclude_config = generate_exclude_config(exclude_map);

        assert_eq!(exclude_config.len(), 1);
        assert!(exclude_config[0].0.is_match("Class1"));
        assert!(exclude_config[0].1.is_match("Title1"));
    }
}
