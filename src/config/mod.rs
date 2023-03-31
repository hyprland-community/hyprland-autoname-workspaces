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
pub struct ConfigFile {
    pub icons: FxHashMap<String, String>,
    #[serde(default)]
    pub title: FxHashMap<String, FxHashMap<String, String>>,
    #[serde(default)]
    pub exclude: FxHashMap<String, String>,
}

impl Config {
    pub fn new() -> Result<Config, Box<dyn Error>> {
        let xdg_dirs = xdg::BaseDirectories::with_prefix("hyprland-autoname-workspaces")?;
        let cfg_path = xdg_dirs.place_config_file("config.toml")?;

        if !cfg_path.exists() {
            _ = create_default_config(&cfg_path);
        }

        let config = read_config_file(&cfg_path)?;

        Ok(Config {
            config: ConfigFile {
                icons: to_uppercase(config.icons, false),
                title: config.title,
                exclude: to_uppercase(config.exclude, true),
            },
            cfg_path,
        })
    }
}

fn read_config_file(cfg_path: &PathBuf) -> Result<ConfigFile, Box<dyn Error>> {
    let mut config_string = fs::read_to_string(cfg_path)?;

    config_string = migrate_config(&config_string, cfg_path)?;

    let config: ConfigFile =
        toml::from_str(&config_string).map_err(|e| format!("Unable to parse: {e:?}"))?;

    Ok(config)
}

fn create_default_config(cfg_path: &PathBuf) -> Result<&'static str, Box<dyn Error + 'static>> {
    let default_config = r#"
[icons]
# Add your icons mapping
# use double quote the key and the value
# take class name from 'hyprctl clients'
"DEFAULT" = ""
"kitty" = "term"
"firefox" = "browser"

[title.kitty]
"neomutt" = "neomutt"

# Add your applications that need to be exclude
# The key is the class, the value is the title.
# You can put an empty title to exclude based on
# class name only, "" make the job.
[exclude]
fcitx = "*"
Steam = "Friends List"
TestApp = ""
"#;

    let mut config_file = File::create(cfg_path)?;
    write!(&mut config_file, "{default_config}")?;
    println!("Default config created in {cfg_path:?}");

    Ok(default_config.trim())
}

pub fn to_uppercase(data: FxHashMap<String, String>, values: bool) -> FxHashMap<String, String> {
    data.into_iter()
        .map(|(k, v)| (k.to_uppercase(), if values { v.to_uppercase() } else { v }))
        .collect()
}

pub fn migrate_config(
    config_string: &str,
    cfg_path: &PathBuf,
) -> Result<String, Box<dyn Error + 'static>> {
    // config file migration if needed
    // can be remove "later" ...
    if !config_string.contains("[icons]") {
        let new_config_string = "[icons]\n".to_owned() + config_string;
        let new_config_string_trimmed = new_config_string.trim();

        fs::write(cfg_path, new_config_string_trimmed)
            .map_err(|e| format!("Cannot migrate config file: {e:?}"))?;
        println!("Config file migrated from v1 to v2");
        return Ok(new_config_string_trimmed.into());
    }

    Ok(config_string.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_uppercase() {
        let mut icons: FxHashMap<String, String> = FxHashMap::default();
        icons.insert("kitty".to_owned(), "kittyicon".to_owned());
        icons = to_uppercase(icons, false);
        assert_eq!(icons.get("kitty"), None);
        assert_eq!(icons.get("KITTY").unwrap(), "kittyicon");

        let mut exclude: FxHashMap<String, String> = FxHashMap::default();
        exclude.insert("Steam".to_owned(), "Friends list".to_owned());
        exclude = to_uppercase(exclude, true);
        assert_eq!(exclude.get("Steam"), None);
        assert_eq!(exclude.get("STEAM").unwrap(), "FRIENDS LIST");
    }

    #[test]
    fn test_create_config_workflow() {
        let cfg_path = &PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
        let config_string = create_default_config(&cfg_path).unwrap();
        let config = read_config_file(&cfg_path);
        assert_eq!(config.unwrap().icons.get("kitty").unwrap(), "term");
        let config_string_legacy = r#"
# Add your icons mapping
# use double quote the key and the value
# take class name from 'hyprctl clients'
"DEFAULT" = ""
"kitty" = "term"
"firefox" = "browser"

[title.kitty]
"neomutt" = "neomutt"

# Add your applications that need to be exclude
# The key is the class, the value is the title.
# You can put an empty title to exclude based on
# class name only, "" make the job.
[exclude]
fcitx = ""
Steam = "Friends List"
"#;
        let config_string_migrated = migrate_config(&config_string_legacy, &cfg_path).unwrap();
        assert_eq!(config_string_migrated.contains("[icons]\n"), true);
        assert_ne!(config_string, config_string_migrated);
        let config_string_migrated_two =
            migrate_config(&config_string_migrated, &cfg_path).unwrap();
        assert_eq!(config_string_migrated, config_string_migrated_two);
    }

    #[test]
    fn test_class_kitty_title_neomutt() {
        let cfg_path = &PathBuf::from("/tmp/hyprland-autoname-workspaces-test.toml");
        let config = read_config_file(&cfg_path);
        assert_eq!(
            config
                .unwrap()
                .title
                .get("kitty")
                .unwrap()
                .get("neomutt")
                .unwrap(),
            "neomutt",
        );
    }
}
