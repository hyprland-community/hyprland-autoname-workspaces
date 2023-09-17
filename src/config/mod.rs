use regex::Regex;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const BIN_NAME: &str = env!("CARGO_BIN_NAME");

#[derive(Default, Clone, Debug)]
pub struct Config {
    pub config: ConfigFile,
    pub cfg_path: Option<PathBuf>,
}

fn default_delim_formatter() -> String {
    " ".to_string()
}

fn default_client_formatter() -> String {
    "{icon}".to_string()
}

fn default_client_active_formatter() -> String {
    "*{icon}*".to_string()
}

fn default_client_fullscreen_formatter() -> String {
    "[{icon}]".to_string()
}

fn default_client_dup_formatter() -> String {
    "{icon}{counter_sup}".to_string()
}

fn default_client_dup_fullscreen_formatter() -> String {
    "[{icon}]{delim}{icon}{counter_unfocused_sup}".to_string()
}

fn default_client_dup_active_formatter() -> String {
    "*{icon}*{delim}{icon}{counter_unfocused_sup}".to_string()
}

fn default_workspace_empty_formatter() -> String {
    "{id}".to_string()
}

fn default_workspace_formatter() -> String {
    "{id}:{delim}{clients}".to_string()
}

fn default_class() -> HashMap<String, String> {
    HashMap::from([("DEFAULT".to_string(), " {class}".to_string())])
}

// Nested serde default doesnt work.
impl Default for ConfigFormatRaw {
    fn default() -> Self {
        toml::from_str("").unwrap()
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct ConfigFormatRaw {
    #[serde(default)]
    pub dedup: bool,
    #[serde(default)]
    pub dedup_inactive_fullscreen: bool,
    #[serde(default = "default_delim_formatter")]
    pub delim: String,
    #[serde(default = "default_workspace_formatter")]
    pub workspace: String,
    #[serde(default = "default_workspace_empty_formatter")]
    pub workspace_empty: String,
    #[serde(default = "default_client_formatter")]
    pub client: String,
    #[serde(default = "default_client_fullscreen_formatter")]
    pub client_fullscreen: String,
    #[serde(default = "default_client_active_formatter")]
    pub client_active: String,
    #[serde(default = "default_client_dup_formatter")]
    pub client_dup: String,
    #[serde(default = "default_client_dup_active_formatter")]
    pub client_dup_active: String,
    #[serde(default = "default_client_dup_fullscreen_formatter")]
    pub client_dup_fullscreen: String,
}

#[derive(Deserialize, Serialize)]
pub struct ConfigFileRaw {
    #[serde(default)]
    pub version: String,
    #[serde(default = "default_class", alias = "icons")]
    pub class: HashMap<String, String>,
    #[serde(default, alias = "active_icons", alias = "icons_active")]
    pub class_active: HashMap<String, String>,
    #[serde(default)]
    pub initial_class: HashMap<String, String>,
    #[serde(default)]
    pub initial_class_active: HashMap<String, String>,
    #[serde(default)]
    pub workspaces_name: HashMap<String, String>,
    #[serde(default, alias = "title_icons")]
    pub title_in_class: HashMap<String, HashMap<String, String>>,
    #[serde(default, alias = "title_active_icons")]
    pub title_in_class_active: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub title_in_initial_class: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub title_in_initial_class_active: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub initial_title_in_class: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub initial_title_in_class_active: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub initial_title_in_initial_class: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub initial_title_in_initial_class_active: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub exclude: HashMap<String, String>,
    #[serde(default)]
    pub format: ConfigFormatRaw,
}

#[derive(Default, Debug, Clone)]
pub struct ConfigFile {
    pub class: Vec<(Regex, String)>,
    pub class_active: Vec<(Regex, String)>,
    pub workspaces_name: Vec<(String, String)>,
    pub initial_class: Vec<(Regex, String)>,
    pub initial_class_active: Vec<(Regex, String)>,
    pub title_in_class: Vec<(Regex, Vec<(Regex, String)>)>,
    pub title_in_class_active: Vec<(Regex, Vec<(Regex, String)>)>,
    pub title_in_initial_class: Vec<(Regex, Vec<(Regex, String)>)>,
    pub title_in_initial_class_active: Vec<(Regex, Vec<(Regex, String)>)>,
    pub initial_title_in_class: Vec<(Regex, Vec<(Regex, String)>)>,
    pub initial_title_in_class_active: Vec<(Regex, Vec<(Regex, String)>)>,
    pub initial_title_in_initial_class: Vec<(Regex, Vec<(Regex, String)>)>,
    pub initial_title_in_initial_class_active: Vec<(Regex, Vec<(Regex, String)>)>,
    pub exclude: Vec<(Regex, Regex)>,
    pub format: ConfigFormatRaw,
}

impl Config {
    pub fn new(
        cfg_path: PathBuf,
        dump_config: bool,
        migrate_config: bool,
    ) -> Result<Config, Box<dyn Error>> {
        if !cfg_path.exists() {
            _ = create_default_config(&cfg_path);
        }

        Ok(Config {
            config: read_config_file(Some(cfg_path.clone()), dump_config, migrate_config)?,
            cfg_path: Some(cfg_path),
        })
    }
}

impl ConfigFileRaw {
    pub fn migrate(&mut self, cfg_path: &Option<PathBuf>) -> Result<(), Box<dyn Error>> {
        self.version = VERSION.to_string();
        let config_updated = toml::to_string(&self)?;
        if let Some(path) = cfg_path {
            let config_file = &mut File::create(path)?;
            write!(config_file, "{config_updated}")?;
            println!("Config file successfully migrated in {path:?}");
        }
        Ok(())
    }
}

pub fn read_config_file(
    cfg_path: Option<PathBuf>,
    dump_config: bool,
    migrate_config: bool,
) -> Result<ConfigFile, Box<dyn Error>> {
    let mut config: ConfigFileRaw = match &cfg_path {
        Some(path) => {
            let config_string = fs::read_to_string(path)?;
            toml::from_str(&config_string).map_err(|e| format!("Unable to parse: {e:?}"))?
        }
        None => toml::from_str("").map_err(|e| format!("Unable to parse: {e:?}"))?,
    };

    migrate_config_file(&mut config, migrate_config, cfg_path)?;

    if dump_config {
        println!("{}", serde_json::to_string_pretty(&config)?);
        process::exit(0);
    }

    Ok(ConfigFile {
        class: generate_icon_config(&config.class),
        class_active: generate_icon_config(&config.class_active),
        workspaces_name: generate_workspaces_name_config(&config.workspaces_name),
        initial_class: generate_icon_config(&config.initial_class),
        initial_class_active: generate_icon_config(&config.initial_class_active),
        title_in_class: generate_title_config(&config.title_in_class),
        title_in_class_active: generate_title_config(&config.title_in_class_active),
        title_in_initial_class: generate_title_config(&config.title_in_initial_class),
        title_in_initial_class_active: generate_title_config(&config.title_in_initial_class_active),
        initial_title_in_class: generate_title_config(&config.initial_title_in_class),
        initial_title_in_class_active: generate_title_config(&config.initial_title_in_class_active),
        initial_title_in_initial_class: generate_title_config(
            &config.initial_title_in_initial_class,
        ),
        initial_title_in_initial_class_active: generate_title_config(
            &config.initial_title_in_initial_class_active,
        ),
        exclude: generate_exclude_config(&config.exclude),
        format: config.format,
    })
}

pub fn get_config_path(args: &Option<String>) -> Result<PathBuf, Box<dyn Error>> {
    let cfg_path = match args {
        Some(path) => PathBuf::from(path),
        _ => {
            let xdg_dirs = xdg::BaseDirectories::with_prefix(BIN_NAME)?;
            xdg_dirs.place_config_file("config.toml")?
        }
    };

    Ok(cfg_path)
}

fn migrate_config_file(
    config: &mut ConfigFileRaw,
    migrate_config: bool,
    cfg_path: Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let default_version = Version::parse("1.0.0")?;
    let actual_version = Version::parse(&config.version).unwrap_or(default_version);
    let last_version = Version::parse(VERSION)?;
    let need_migrate = actual_version < last_version;
    if need_migrate {
        println!("Config in version {actual_version} need to be updated in version {last_version}, run: {BIN_NAME} --migrate-config");
    }
    if need_migrate && migrate_config {
        config
            .migrate(&cfg_path)
            .map_err(|e| format!("Unable to migrate config {e:?}"))?;
    };
    Ok(())
}

pub fn create_default_config(cfg_path: &PathBuf) -> Result<&'static str, Box<dyn Error + 'static>> {
    // TODO: maybe we should dump the config from the default values of the struct?
    let default_config = r#"
version = "1.1.10"

# [format]
# Deduplicate icons if enable.
# A superscripted counter will be added.
# dedup = false
# dedup_inactive_fullscreen = false # dedup more
# window delimiter
# delim = " "

# available formatter:
# {counter_sup} - superscripted count of clients on the workspace, and simple {counter}, {delim}
# {icon}, {client}
# workspace formatter
# workspace = "{id}:{delim}{clients}" # {id}, {delim} and {clients} are supported
# workspace_empty = "{id}" # {id}, {delim} and {clients} are supported
# client formatter
# client = "{icon}"
# client_active = "*{icon}*"

# deduplicate client formatter
# client_fullscreen = "[{icon}]"
# client_dup = "{client}{counter_sup}"
# client_dup_fullscreen = "[{icon}]{delim}{icon}{counter_unfocused}"
# client_dup_active = "*{icon}*{delim}{icon}{counter_unfocused}"

[class]
# Add your icons mapping
# use double quote the key and the value
# take class name from 'hyprctl clients'
"DEFAULT" = " {class}: {title}"
"(?i)Kitty" = "term"
"[Ff]irefox" = "browser"
"(?i)waydroid.*" = "droid"

[class_active]
DEFAULT = "*{icon}*"
"(?i)ExampleOneTerm" = "<span foreground='red'>{icon}</span>"

# [initial_class]
# "DEFAULT" = " {class}: {title}"
# "(?i)Kitty" = "term"

# [initial_class_active]
# "(?i)Kitty" = "*TERM*"

[title_in_class."(?i)kitty"]
"(?i)neomutt" = "neomutt"
# regex captures support is supported
# "emerge: (.+?/.+?)-.*" = "{match1}"

[title_in_class_active."(?i)firefox"]
"(?i)twitch" = "<span color='purple'>{icon}</span>"

# [title_in_initial_class."(?i)kitty"]
# "(?i)neomutt" = "neomutt"

# [initial_title_in_class."(?i)kitty"]
# "(?i)neomutt" = "neomutt"

# [initial_title_in_initial_class."(?i)kitty"]
# "(?i)neomutt" = "neomutt"

# [initial_title."(?i)kitty"]
# "zsh" = "Zsh"

# [initial_title_active."(?i)kitty"]
# "zsh" = "*Zsh*"

# Add your applications that need to be exclude
# The key is the class, the value is the title.
# You can put an empty title to exclude based on
# class name only, "" make the job.
[exclude]
"" = "^$" # prevent displaying icon for empty class
"(?i)fcitx" = ".*" # will match all title for fcitx
"(?i)TestApp" = "" # will match all title for TestApp
aProgram = "^$" # will match null title for aProgram
"[Ss]team" = "^(Friends List.*)?$" # will match Steam friends list plus all popups (empty titles)

[workspaces_name]
0 = "zero"
1 = "one"
2 = "two"
3 = "three"
4 = "four"
5 = "five"
6 = "six"
7 = "seven"
8 = "eight"
9 = "nine"
10 = "ten"

"#
    .trim();

    let mut config_file = File::create(cfg_path)?;
    write!(&mut config_file, "{default_config}")?;
    println!("Default config created in {cfg_path:?}");

    Ok(default_config)
}

/// Creates a Regex from a given pattern and logs an error if the pattern is invalid.
///
/// # Arguments
///
/// * `pattern` - A string representing the regex pattern to be compiled.
///
/// # Returns
///
/// * `Option<Regex>` - Returns Some(Regex) if the pattern is valid, otherwise None.
///
/// # Example
///
/// ```
/// use regex::Regex;
/// use crate::regex_with_error_logging;
///
/// let valid_pattern = "Class1";
/// let invalid_pattern = "Class1[";
///
/// assert!(regex_with_error_logging(valid_pattern).is_some());
/// assert!(regex_with_error_logging(invalid_pattern).is_none());
/// ```
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
    icons: &HashMap<String, HashMap<String, String>>,
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
fn generate_icon_config(icons: &HashMap<String, String>) -> Vec<(Regex, String)> {
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
fn generate_exclude_config(icons: &HashMap<String, String>) -> Vec<(Regex, Regex)> {
    icons
        .iter()
        .filter_map(|(class, title)| {
            regex_with_error_logging(class).and_then(|re_class| {
                regex_with_error_logging(title).map(|re_title| (re_class, re_title))
            })
        })
        .collect()
}

/// Generates the workspaces id to name mapping
fn generate_workspaces_name_config(
    workspaces_name: &HashMap<String, String>,
) -> Vec<(String, String)> {
    workspaces_name
        .iter()
        .filter_map(|(id, name)| {
            if id.parse::<i32>().is_ok() {
                Some((id.to_string(), name.to_string()))
            } else {
                None
            }
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

        let title_config = generate_title_config(&title_icons_map);

        assert_eq!(title_config.len(), 1);
        assert!(title_config[0].0.is_match("Class1"));
        assert_eq!(title_config[0].1.len(), 1);
        assert!(title_config[0].1[0].0.is_match("Title1"));
        assert_eq!(title_config[0].1[0].1, "Icon1");
    }

    #[test]
    fn test_generate_icon_config() {
        let mut list_class: HashMap<String, String> = HashMap::new();
        list_class.insert("Class1".to_string(), "Icon1".to_string());

        let icons_config = generate_icon_config(&list_class);

        assert_eq!(icons_config.len(), 1);
        assert!(icons_config[0].0.is_match("Class1"));
        assert_eq!(icons_config[0].1, "Icon1");
    }

    #[test]
    fn test_generate_exclude_config() {
        let mut list_exclude: HashMap<String, String> = HashMap::new();
        list_exclude.insert("Class1".to_string(), "Title1".to_string());

        let exclude_config = generate_exclude_config(&list_exclude);

        assert_eq!(exclude_config.len(), 1);
        assert!(exclude_config[0].0.is_match("Class1"));
        assert!(exclude_config[0].1.is_match("Title1"));
    }

    #[test]
    fn test_regex_with_error_logging() {
        let valid_pattern = "Class1";
        let invalid_pattern = "Class1[";

        assert!(regex_with_error_logging(valid_pattern).is_some());
        assert!(regex_with_error_logging(invalid_pattern).is_none());
    }

    #[test]
    fn test_config_new_and_read_again_then_compare_format() {
        let cfg_path = PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
        let config = Config::new(cfg_path.clone(), false, false);
        assert_eq!(config.is_ok(), true);
        let config = config.unwrap().clone();
        assert_eq!(config.cfg_path.clone(), Some(cfg_path.clone()));
        let format = config.config.format.clone();
        let config2 = read_config_file(Some(cfg_path.clone()), false, false).unwrap();
        let format2 = config2.format.clone();
        assert_eq!(format, format2);
    }
}
