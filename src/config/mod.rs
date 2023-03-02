use rustc_hash::FxHashMap;
use serde::Deserialize;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
#[macro_use]
mod macros;

pub struct Config {
    pub config: ConfigFile,
    pub cfg_path: PathBuf,
}

#[derive(Deserialize)]
pub struct ConfigFile {
    pub icons: FxHashMap<String, String>,
    #[serde(default)]
    pub exclude: FxHashMap<String, String>,
}

impl Config {
    pub fn new() -> Result<Config, Box<dyn Error>> {
        let mut config_file: File;
        let xdg_dirs = xdg::BaseDirectories::with_prefix("hyprland-autoname-workspaces")?;
        let cfg_path = xdg_dirs.place_config_file("config.toml")?;
        if !cfg_path.exists() {
            config_file = File::create(&cfg_path)?;
            let default_config = r#"[icons]
# Add your icons mapping
# use double quote the key and the value
# take class name from 'hyprctl clients'
"DEFAULT" = ""
"kitty" = "term"
"firefox" = "browser"

# Add your applications that need to be exclude
# You can put what you want as value, "" make the job.
[exclude]
fcitx5 = ""
fcitx = ""
"#;
            write!(&mut config_file, "{default_config}")?;
            println!("Default config created in {cfg_path:?}");
        }
        let mut config_string = fs::read_to_string(cfg_path.clone())?;

        // config file migration if needed
        // can be remove "later" ...
        if !config_string.contains("[icons]") {
            config_string = "[icons]\n".to_owned() + &config_string;
            fs::write(&cfg_path, &config_string)
                .map_err(|e| format!("Cannot migrate config file: {e:?}"))?;
            println!("Config file migrated from v1 to v2");
        }

        let config: ConfigFile =
            toml::from_str(&config_string).map_err(|e| format!("Unable to parse: {e:?}"))?;

        uppercase_keys_of!(config, icons, exclude);

        Ok(Config {
            config: ConfigFile { icons, exclude },
            cfg_path,
        })
    }
}
