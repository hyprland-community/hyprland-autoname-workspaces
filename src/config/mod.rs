use regex::Regex;
use rustc_hash::FxHashMap;
use serde::Deserialize;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

pub struct Config {
    pub config: ConfigFile,
    pub cfg_path: PathBuf,
}

#[derive(Deserialize)]
pub struct ConfigFileRaw {
    pub icons: FxHashMap<String, String>,
    #[serde(default)]
    pub title: FxHashMap<String, FxHashMap<String, String>>,
    #[serde(default)]
    pub exclude: FxHashMap<String, String>,
}

pub struct ConfigFile {
    pub icons: Vec<(Regex, String)>,
    pub title: Vec<(Regex, Vec<(Regex, String)>)>,
    pub exclude: Vec<(Regex, Regex)>,
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

fn log_regex_error<T>(e: regex::Error) -> Option<T> {
    println!("Unable to parse regex: {e:?}");
    None
}

pub fn read_config_file(cfg_path: &PathBuf) -> Result<ConfigFile, Box<dyn Error>> {
    let config_string = fs::read_to_string(cfg_path)?;

    let config: ConfigFileRaw =
        toml::from_str(&config_string).map_err(|e| format!("Unable to parse: {e:?}"))?;

    let icons = config
        .icons
        .iter()
        .filter_map(|(class, icon)| match Regex::new(class) {
            Ok(class) => Some((class, icon.to_string())),
            Err(e) => log_regex_error(e),
        })
        .collect();

    let title = config
        .title
        .iter()
        .filter_map(|(class, title_icon)| match Regex::new(class) {
            Ok(class) => Some((
                class,
                title_icon
                    .iter()
                    .filter_map(|(title, icon)| match Regex::new(title) {
                        Ok(title) => Some((title, icon.to_string())),
                        Err(e) => log_regex_error(e),
                    })
                    .collect(),
            )),
            Err(e) => log_regex_error(e),
        })
        .collect();

    let exclude = config
        .exclude
        .iter()
        .filter_map(|(class, title)| match Regex::new(class) {
            Ok(re_class) => match Regex::new(title) {
                Ok(re_title) => Some((re_class, re_title)),
                Err(e) => log_regex_error(e),
            },
            Err(e) => log_regex_error(e),
        })
        .collect();

    Ok(ConfigFile {
        icons,
        title,
        exclude,
    })
}

pub fn create_default_config(cfg_path: &PathBuf) -> Result<&'static str, Box<dyn Error + 'static>> {
    let default_config = r#"
[icons]
# Add your icons mapping
# use double quote the key and the value
# take class name from 'hyprctl clients'
"DEFAULT" = ""
"(?i)Kitty" = "term"
"[Ff]irefox" = "browser"
"(?i)waydroid.*" = "droid"

[title."(?i)kitty"]
"(?i)neomutt" = "neomutt"


# Add your applications that need to be exclude
# The key is the class, the value is the title.
# You can put an empty title to exclude based on
# class name only, "" make the job.
[exclude]
fcitx = ".*"
"[Ss]team" = "Friends List"
"(?i)TestApp" = ""
"#;

    let mut config_file = File::create(cfg_path)?;
    write!(&mut config_file, "{default_config}")?;
    println!("Default config created in {cfg_path:?}");

    Ok(default_config.trim())
}
