use crate::renamer::IconConfig::*;
use crate::renamer::{ConfigFile, Renamer};

type Rule = String;
type Icon = String;
type Title = String;
type Class = String;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IconConfig {
    ActiveClass(Rule, Icon),
    ActiveInitialClass(Rule, Icon),
    ActiveTitleInInitialClass(Rule, Icon),
    ActiveInitialTitleInClass(Rule, Icon),
    ActiveInitialTitleInInitialClass(Rule, Icon),
    ActiveTitleInClass(Rule, Icon),
    Class(Rule, Icon),
    InitialClass(Rule, Icon),
    TitleInClass(Rule, Icon),
    TitleInInitialClass(Rule, Icon),
    InitialTitleInClass(Rule, Icon),
    InitialTitleInInitialClass(Rule, Icon),
    ActiveDefault(Icon),
    Default(Icon),
}

impl IconConfig {
    pub fn icon(&self) -> Icon {
        match &self {
            Default(icon)
            | ActiveDefault(icon)
            | Class(_, icon)
            | InitialClass(_, icon)
            | TitleInClass(_, icon)
            | TitleInInitialClass(_, icon)
            | InitialTitleInClass(_, icon)
            | InitialTitleInInitialClass(_, icon)
            | ActiveClass(_, icon)
            | ActiveInitialClass(_, icon)
            | ActiveTitleInInitialClass(_, icon)
            | ActiveInitialTitleInInitialClass(_, icon)
            | ActiveInitialTitleInClass(_, icon)
            | ActiveTitleInClass(_, icon) => icon.to_string(),
        }
    }

    pub fn rule(&self) -> Rule {
        match &self {
            Class(rule, _)
            | InitialClass(rule, _)
            | TitleInClass(rule, _)
            | TitleInInitialClass(rule, _)
            | InitialTitleInClass(rule, _)
            | InitialTitleInInitialClass(rule, _)
            | ActiveClass(rule, _)
            | ActiveInitialClass(rule, _)
            | ActiveTitleInInitialClass(rule, _)
            | ActiveInitialTitleInInitialClass(rule, _)
            | ActiveInitialTitleInClass(rule, _)
            | ActiveTitleInClass(rule, _) => rule.to_string(),
            _ => unreachable!(),
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
    ) -> Option<IconConfig> {
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

        list_initial_title_in_initial_class
            .iter()
            .find(|(re_class, _)| re_class.is_match(initial_class))
            .and_then(|(_, title_icon)| {
                title_icon
                    .iter()
                    .find(|(rule, _)| rule.is_match(initial_title))
                    .map(|(rule, icon)| {
                        if is_active {
                            IconConfig::ActiveInitialTitleInInitialClass(
                                rule.to_string(),
                                icon.to_string(),
                            )
                        } else {
                            IconConfig::InitialTitleInInitialClass(
                                rule.to_string(),
                                icon.to_string(),
                            )
                        }
                    })
            })
            .or_else(|| {
                list_initial_title_in_class
                    .iter()
                    .find(|(re_class, _)| re_class.is_match(class))
                    .and_then(|(_, title_icon)| {
                        title_icon
                            .iter()
                            .find(|(rule, _)| rule.is_match(initial_title))
                            .map(|(rule, icon)| {
                                if is_active {
                                    IconConfig::ActiveInitialTitleInClass(
                                        rule.to_string(),
                                        icon.to_string(),
                                    )
                                } else {
                                    IconConfig::InitialTitleInClass(
                                        rule.to_string(),
                                        icon.to_string(),
                                    )
                                }
                            })
                    })
                    .or_else(|| {
                        list_title_in_initial_class
                            .iter()
                            .find(|(re_class, _)| re_class.is_match(initial_class))
                            .and_then(|(_, title_icon)| {
                                title_icon
                                    .iter()
                                    .find(|(rule, _)| rule.is_match(title))
                                    .map(|(rule, icon)| {
                                        if is_active {
                                            IconConfig::ActiveTitleInInitialClass(
                                                rule.to_string(),
                                                icon.to_string(),
                                            )
                                        } else {
                                            IconConfig::TitleInInitialClass(
                                                rule.to_string(),
                                                icon.to_string(),
                                            )
                                        }
                                    })
                            })
                            .or_else(|| {
                                list_title_in_class
                                    .iter()
                                    .find(|(re_class, _)| re_class.is_match(class))
                                    .and_then(|(_, title_icon)| {
                                        title_icon
                                            .iter()
                                            .find(|(rule, _)| rule.is_match(title))
                                            .map(|(rule, icon)| {
                                                if is_active {
                                                    IconConfig::ActiveTitleInClass(
                                                        rule.to_string(),
                                                        icon.to_string(),
                                                    )
                                                } else {
                                                    IconConfig::TitleInClass(
                                                        rule.to_string(),
                                                        icon.to_string(),
                                                    )
                                                }
                                            })
                                    })
                                    .or_else(|| {
                                        list_initial_class
                                            .iter()
                                            .find(|(rule, _)| rule.is_match(initial_class))
                                            .map(|(rule, icon)| {
                                                if is_active {
                                                    IconConfig::ActiveInitialClass(
                                                        rule.to_string(),
                                                        icon.to_string(),
                                                    )
                                                } else {
                                                    IconConfig::InitialClass(
                                                        rule.to_string(),
                                                        icon.to_string(),
                                                    )
                                                }
                                            })
                                    })
                                    .or_else(|| {
                                        list_class
                                            .iter()
                                            .find(|(rule, _)| rule.is_match(class))
                                            .map(|(rule, icon)| {
                                                if is_active {
                                                    IconConfig::ActiveClass(
                                                        rule.to_string(),
                                                        icon.to_string(),
                                                    )
                                                } else {
                                                    IconConfig::Class(
                                                        rule.to_string(),
                                                        icon.to_string(),
                                                    )
                                                }
                                            })
                                    })
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
    ) -> IconConfig {
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
            .unwrap_or(IconConfig::Default("no icon".to_string()));

        let icon_default_active = self
            .find_icon("DEFAULT", "DEFAULT", "", "", true, config)
            .unwrap_or({
                self.find_icon("DEFAULT", "DEFAULT", "", "", false, config)
                    .map(|i| IconConfig::ActiveClass(i.rule(), i.icon()))
                    .unwrap_or(IconConfig::ActiveDefault("no icon".to_string()))
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
