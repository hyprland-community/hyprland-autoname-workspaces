/// Generates a configuration for title and title_active.
///
/// This macro processes the given configuration field and creates a collection
/// of tuples, where each tuple contains a compiled regex and a corresponding
/// collection of tuples with a compiled regex and an icon string.
///
/// # Arguments
///
/// * `$config_field` - The configuration field to process (either config.title or config.title_active).
macro_rules! generate_title_config {
    ($config_field:expr) => {{
        $config_field
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
    }};
}

/// Generates a configuration for icons and icons_active.
///
/// This macro processes the given configuration field and creates a collection
/// of tuples, where each tuple contains a compiled regex and a corresponding icon string.
///
/// # Arguments
///
/// * `$config_field` - The configuration field to process (either config.icons or config.icons_active).
macro_rules! generate_icon_config {
    ($config_field:expr) => {{
        $config_field
            .iter()
            .filter_map(|(class, icon)| {
                regex_with_error_logging(class).map(|re| (re, icon.to_string()))
            })
            .collect()
    }};
}

/// Generates a configuration for exclude.
///
/// This macro processes the given configuration field and creates a collection
/// of tuples, where each tuple contains two compiled regexes, one for class and one for title.
///
/// # Arguments
///
/// * `$config_field` - The configuration field to process (config.exclude).
macro_rules! generate_exclude_config {
    ($config_field:expr) => {{
        $config_field
            .iter()
            .filter_map(|(class, title)| {
                regex_with_error_logging(class).and_then(|re_class| {
                    regex_with_error_logging(title).map(|re_title| (re_class, re_title))
                })
            })
            .collect()
    }};
}
