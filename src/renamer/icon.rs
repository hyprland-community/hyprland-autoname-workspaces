use std::collections::HashMap;

use crate::renamer::IconConfig::*;
use crate::renamer::IconStatus::*;
use crate::renamer::{ConfigFile, Renamer};

type Rule = String;
type Icon = String;
type Title = String;
type Class = String;
type Captures = Option<HashMap<String, String>>;
type IconMatch = Option<(Rule, Icon, Option<HashMap<String, String>>)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconConfig {
    Class(Rule, Icon),
    InitialClass(Rule, Icon),
    TitleInClass(Rule, Icon, Captures),
    TitleInInitialClass(Rule, Icon, Captures),
    InitialTitleInClass(Rule, Icon, Captures),
    InitialTitleInInitialClass(Rule, Icon, Captures),
    Default(Icon),
}

impl IconConfig {
    pub fn icon(&self) -> Icon {
        let (_, icon, _) = self.get();
        icon
    }

    pub fn rule(&self) -> Rule {
        let (rule, _, _) = self.get();
        rule
    }

    pub fn captures(&self) -> Captures {
        let (_, _, captures) = self.get();
        captures
    }

    pub fn get(&self) -> (Rule, Icon, Captures) {
        match &self {
            Default(icon) => ("DEFAULT".to_string(), icon.to_string(), None),
            Class(rule, icon) | InitialClass(rule, icon) => {
                (rule.to_string(), icon.to_string(), None)
            }
            TitleInClass(rule, icon, captures)
            | TitleInInitialClass(rule, icon, captures)
            | InitialTitleInClass(rule, icon, captures)
            | InitialTitleInInitialClass(rule, icon, captures) => {
                (rule.to_string(), icon.to_string(), captures.clone())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconStatus {
    Active(IconConfig),
    Inactive(IconConfig),
}

impl IconStatus {
    pub fn icon(&self) -> Icon {
        match self {
            Active(config) | Inactive(config) => config.icon(),
        }
    }

    pub fn rule(&self) -> Rule {
        match self {
            Active(config) | Inactive(config) => config.rule(),
        }
    }

    pub fn captures(&self) -> Captures {
        match self {
            Active(config) | Inactive(config) => config.captures(),
        }
    }
}

macro_rules! find_icon_config {
    ($list:expr, $class:expr, $title:expr, $is_active:expr, $enum_variant:ident) => {
        find_title_in_class_helper($list, $class, $title).map(|(rule, icon, captures)| {
            if $is_active {
                Active($enum_variant(rule, icon, captures))
            } else {
                Inactive($enum_variant(rule, icon, captures))
            }
        })
    };
    ($list:expr, $class:expr, $is_active:expr, $enum_variant:ident) => {
        find_class_helper($list, $class).map(|(rule, icon)| {
            if $is_active {
                Active($enum_variant(rule, icon))
            } else {
                Inactive($enum_variant(rule, icon))
            }
        })
    };
}

impl Renamer {
    fn find_icon(
        &self,
        initial_class: &str,
        class: &str,
        initial_title: &str,
        title: &str,
        is_active: bool,
        config: &ConfigFile,
    ) -> Option<IconStatus> {
        let (
            list_initial_title_in_initial_class,
            list_initial_title_in_class,
            list_title_in_initial_class,
            list_title_in_class,
            list_initial_class,
            list_class,
        ) = if is_active {
            (
                &config.initial_title_in_initial_class_active,
                &config.initial_title_in_class_active,
                &config.title_in_initial_class_active,
                &config.title_in_class_active,
                &config.initial_class_active,
                &config.class_active,
            )
        } else {
            (
                &config.initial_title_in_initial_class,
                &config.initial_title_in_class,
                &config.title_in_initial_class,
                &config.title_in_class,
                &config.initial_class,
                &config.class,
            )
        };

        find_icon_config!(
            list_initial_title_in_initial_class,
            initial_class,
            initial_title,
            is_active,
            InitialTitleInInitialClass
        )
        .or_else(|| {
            find_icon_config!(
                list_initial_title_in_class,
                class,
                initial_title,
                is_active,
                InitialTitleInClass
            )
            .or_else(|| {
                find_icon_config!(
                    list_title_in_initial_class,
                    initial_class,
                    title,
                    is_active,
                    TitleInInitialClass
                )
                .or_else(|| {
                    find_icon_config!(list_title_in_class, class, title, is_active, TitleInClass)
                        .or_else(|| {
                            find_icon_config!(list_initial_class, class, is_active, InitialClass)
                        })
                        .or_else(|| find_icon_config!(list_class, class, is_active, Class))
                })
            })
        })
    }

    pub fn parse_icon(
        &self,
        initial_class: Class,
        class: Class,
        initial_title: Title,
        title: Title,
        is_active: bool,
        config: &ConfigFile,
    ) -> IconStatus {
        let icon = self.find_icon(
            &initial_class,
            &class,
            &initial_title,
            &title,
            false,
            config,
        );

        let icon_active =
            self.find_icon(&initial_class, &class, &initial_title, &title, true, config);

        let icon_default = self
            .find_icon("DEFAULT", "DEFAULT", "", "", false, config)
            .unwrap_or(Inactive(Default("no icon".to_string())));

        let icon_default_active = self
            .find_icon("DEFAULT", "DEFAULT", "", "", true, config)
            .unwrap_or({
                self.find_icon("DEFAULT", "DEFAULT", "", "", false, config)
                    .map(|i| Active(Class(i.rule(), i.icon())))
                    .unwrap_or(Active(Default("no icon".to_string())))
            });

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

fn find_title_in_class_helper(
    list: &[(regex::Regex, Vec<(regex::Regex, Icon)>)],
    class: &str,
    title: &str,
) -> IconMatch {
    list.iter()
        .find(|(re_class, _)| re_class.is_match(class))
        .and_then(|(_, title_icon)| {
            title_icon
                .iter()
                .find(|(rule, _)| rule.is_match(title))
                .map(|(rule, icon)| (rule, icon))
        })
        .map(|(rule, icon)| match rule.captures(title) {
            Some(re_captures) => (
                rule.to_string(),
                icon.to_string(),
                Some(
                    re_captures
                        .iter()
                        .enumerate()
                        .map(|(k, v)| {
                            (
                                format!("match{k}"),
                                v.map_or("", |m| m.as_str()).to_string(),
                            )
                        })
                        .collect(),
                ),
            ),
            None => (rule.to_string(), icon.to_string(), None),
        })
}

fn find_class_helper(list: &[(regex::Regex, Icon)], class: &str) -> Option<(Rule, Icon)> {
    list.iter()
        .find(|(rule, _)| rule.is_match(class))
        .map(|(rule, icon)| (rule.to_string(), icon.to_string()))
}
