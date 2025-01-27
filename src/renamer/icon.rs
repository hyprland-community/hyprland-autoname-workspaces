use crate::renamer::IconConfig::*;
use crate::renamer::IconStatus::*;
use crate::renamer::{ConfigFile, Renamer};
use std::collections::HashMap;

type Rule = String;
type Icon = String;
type Title = String;
type Class = String;
type Captures = Option<HashMap<String, String>>;
type ListTitleInClass<'a> = Option<&'a [(regex::Regex, Vec<(regex::Regex, Icon)>)]>;
type ListClass<'a> = Option<&'a [(regex::Regex, Icon)]>;

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

    pub fn captures(&self) -> Captures {
        match self {
            Active(config) | Inactive(config) => config.captures(),
        }
    }
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

        find_icon_helper(
            is_active,
            Some(list_initial_title_in_initial_class),
            None,
            IconParams {
                class: None,
                title: None,
                initial_class: Some(initial_class),
                initial_title: Some(initial_title),
            },
        )
        .or(find_icon_helper(
            is_active,
            Some(list_initial_title_in_class),
            None,
            IconParams {
                class: Some(class),
                title: None,
                initial_class: None,
                initial_title: Some(initial_title),
            },
        )
        .or(find_icon_helper(
            is_active,
            Some(list_title_in_initial_class),
            None,
            IconParams {
                class: None,
                title: Some(title),
                initial_class: Some(initial_class),
                initial_title: None,
            },
        )
        .or(find_icon_helper(
            is_active,
            Some(list_title_in_class),
            None,
            IconParams {
                class: Some(class),
                title: Some(title),
                initial_class: None,
                initial_title: None,
            },
        )
        .or(find_icon_helper(
            is_active,
            None,
            Some(list_initial_class),
            IconParams {
                class: None,
                title: None,
                initial_class: Some(initial_class),
                initial_title: None,
            },
        ))
        .or(find_icon_helper(
            is_active,
            None,
            Some(list_class),
            IconParams {
                class: Some(class),
                title: None,
                initial_class: None,
                initial_title: None,
            },
        )))))
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
            .unwrap_or(icon_default.clone());

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

pub struct IconParams<'a> {
    class: Option<&'a str>,
    title: Option<&'a str>,
    initial_class: Option<&'a str>,
    initial_title: Option<&'a str>,
}

pub fn forge_icon_status(
    is_active: bool,
    rule: String,
    icon: String,
    params: IconParams,
    captures: Captures,
) -> IconStatus {
    let icon = match (
        params.class,
        params.title,
        params.initial_class,
        params.initial_title,
        captures,
    ) {
        (None, None, None, None, None) => Default(icon),
        (Some(_), None, None, None, None) => Class(rule, icon),
        (None, None, Some(_), None, None) => InitialClass(rule, icon),
        (Some(_), Some(_), None, None, c) => TitleInClass(rule, icon, c),
        (None, None, Some(_), Some(_), c) => InitialTitleInInitialClass(rule, icon, c),
        (None, Some(_), Some(_), None, c) => TitleInInitialClass(rule, icon, c),
        (Some(_), None, None, Some(_), c) => InitialTitleInClass(rule, icon, c),
        (_, _, _, _, _) => Default(icon),
    };

    if is_active {
        Active(icon)
    } else {
        Inactive(icon)
    }
}

fn find_icon_helper(
    is_active: bool,
    list_title_in_class: ListTitleInClass,
    list_class: ListClass,
    params: IconParams,
) -> Option<IconStatus> {
    let the_class = match (params.class, params.initial_class) {
        (Some(c), None) | (None, Some(c)) => c,
        (_, _) => unreachable!(),
    };

    match (list_class, list_title_in_class) {
        (Some(list), None) => {
            list.iter()
                .find(|(rule, _)| rule.is_match(the_class))
                .map(|(rule, icon)| {
                    forge_icon_status(is_active, rule.to_string(), icon.to_string(), params, None)
                })
        }
        (None, Some(list)) => {
            let the_title = match (params.title, params.initial_title) {
                (Some(t), None) | (None, Some(t)) => t,
                (_, _) => unreachable!(),
            };

            list.iter()
                .find(|(re_class, _)| re_class.is_match(the_class))
                .and_then(|(_, title_icon)| {
                    title_icon
                        .iter()
                        .find(|(rule, _)| rule.is_match(the_title))
                        .map(|(rule, icon)| {
                            forge_icon_status(
                                is_active,
                                rule.to_string(),
                                icon.to_string(),
                                params,
                                get_captures(Some(the_title), rule),
                            )
                        })
                })
        }
        (_, _) => unreachable!(),
    }
}

fn get_captures(title: Option<&str>, rule: &regex::Regex) -> Captures {
    match title {
        Some(t) => rule.captures(t).map(|re_captures| {
            re_captures
                .iter()
                .enumerate()
                .map(|(k, v)| {
                    (
                        format!("match{k}"),
                        v.map_or("", |m| m.as_str()).to_string(),
                    )
                })
                .collect()
        }),
        _ => None,
    }
}
