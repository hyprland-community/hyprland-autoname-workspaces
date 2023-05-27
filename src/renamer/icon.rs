use crate::renamer::IconConfig::*;
use crate::renamer::{ConfigFile, Renamer};

type Rule = String;
type Icon = String;
type Title = String;
type Class = String;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IconConfig {
    ActiveClass(Rule, Icon),
    ActiveTitle(Rule, Icon),
    ActiveDefault(Icon),
    Class(Rule, Icon),
    Title(Rule, Icon),
    Default(Icon),
}

impl IconConfig {
    pub fn icon(&self) -> Icon {
        match &self {
            Default(icon) | ActiveDefault(icon) => icon.to_string(),
            Title(_, icon) | Class(_, icon) => icon.to_string(),
            ActiveClass(_, icon) | ActiveTitle(_, icon) => icon.to_string(),
        }
    }
}

impl Renamer {
    fn find_icon(
        &self,
        class: &str,
        title: &str,
        is_active: bool,
        config: &ConfigFile,
    ) -> Option<IconConfig> {
        let (list_title, list_class) = if is_active {
            (&config.title_active, &config.icons_active)
        } else {
            (&config.title, &config.icons)
        };

        list_title
            .iter()
            .find(|(re_class, _)| re_class.is_match(class))
            .and_then(|(_, title_icon)| {
                title_icon
                    .iter()
                    .find(|(rule, _)| rule.is_match(title))
                    .map(|(rule, icon)| {
                        if is_active {
                            IconConfig::ActiveTitle(rule.to_string(), icon.to_string())
                        } else {
                            IconConfig::Title(rule.to_string(), icon.to_string())
                        }
                    })
            })
            .or_else(|| {
                list_class
                    .iter()
                    .find(|(rule, _)| rule.is_match(class))
                    .map(|(rule, icon)| {
                        if is_active {
                            IconConfig::ActiveClass(rule.to_string(), icon.to_string())
                        } else {
                            IconConfig::Class(rule.to_string(), icon.to_string())
                        }
                    })
            })
    }

    pub fn parse_icon(
        &self,
        class: Class,
        title: Title,
        is_active: bool,
        config: &ConfigFile,
    ) -> IconConfig {
        let icon = self.find_icon(&class, &title, false, config);
        let icon_active = self.find_icon(&class, &title, true, config);
        let icon_default = self
            .find_icon("DEFAULT", "", false, config)
            .unwrap_or(IconConfig::Default("no icon".to_string()));
        let icon_default_active = self
            .find_icon("DEFAULT", "", true, config)
            .unwrap_or(IconConfig::ActiveDefault("no icon".to_string()));

        if is_active {
            icon_active.unwrap_or(match icon {
                Some(i) => i,
                None => icon_default_active,
            })
        } else {
            icon.unwrap_or_else(|| {
                if self.args.verbose {
                    println!("- window: class '{}' need a shiny icon", class);
                }
                icon_default
            })
        }
    }
}
