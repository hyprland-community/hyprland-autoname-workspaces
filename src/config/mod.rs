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
    #[serde(default)]
    pub format: FxHashMap<String, String>,
}

pub struct ConfigFile {
    pub icons: Vec<(Regex, String)>,
    pub title: Vec<(Regex, Vec<(Regex, String)>)>,
    pub exclude: Vec<(Regex, Regex)>,
    pub format: FxHashMap<String, String>,
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

fn regex_with_error_logging(pattern: &str) -> Option<Regex> {
    match Regex::new(pattern) {
        Ok(re) => Some(re),
        Err(e) => {
            println!("Unable to parse regex: {e:?}");
            None
        }
    }
}

pub fn read_config_file(cfg_path: &PathBuf) -> Result<ConfigFile, Box<dyn Error>> {
    let config_string = fs::read_to_string(cfg_path)?;

    let config: ConfigFileRaw =
        toml::from_str(&config_string).map_err(|e| format!("Unable to parse: {e:?}"))?;

    let format = config.format;

    let icons = config
        .icons
        .iter()
        .filter_map(|(class, icon)| {
            regex_with_error_logging(class).map(|re| (re, icon.to_string()))
        })
        .collect();

    let title = config
        .title
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
        .collect();

    let exclude = config
        .exclude
        .iter()
        .filter_map(|(class, title)| {
            regex_with_error_logging(class).and_then(|re_class| {
                regex_with_error_logging(title).map(|re_title| (re_class, re_title))
            })
        })
        .collect();

    Ok(ConfigFile {
        icons,
        title,
        exclude,
        format,
    })
}

pub fn create_default_config(cfg_path: &PathBuf) -> Result<&'static str, Box<dyn Error + 'static>> {
    let default_config = r#"
# Deduplicate icons if enable.
# A superscripted counter will be added.

[format]
delim = " "
dedup = false

[icons]
# Add your icons mapping
# use double quote the key and the value
# take class name from 'hyprctl clients'
"DEFAULT" = " ${class}: ${title}"
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
"(?i)fcitx" = ".*" # will match all title for fcitx
"(?i)TestApp" = "" # will match all title for TestApp
aProgram = "^$" # will match null title for aProgram
"[Ss]team" = "Friends List.*"
"[Ss]team" = "^$" # will match all Steam window with null title (some popup)
"#;

    let mut config_file = File::create(cfg_path)?;
    write!(&mut config_file, "{default_config}")?;
    println!("Default config created in {cfg_path:?}");

    Ok(default_config.trim())
}
