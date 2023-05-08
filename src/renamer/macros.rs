/// Renames the workspace if the given events occur.
///
/// # Arguments
///
/// * `$self` - The main struct containing the renameworkspace method.
/// * `$ev` - The event manager to attach event handlers.
/// * `$x` - A list of events to attach the handlers to.
macro_rules! rename_workspace_if {
    ( $self: ident, $ev: ident, $( $x:ident ), * ) => {
        $(
        let this = $self.clone();
        $ev.$x(move |_, _| _ = this.renameworkspace());
        )*
    };
}

/// Formats a string by replacing the placeholders with values from the given HashMap.
///
/// # Arguments
///
/// * `$fmt` - The format string containing placeholders wrapped in curly braces, e.g., "{title}".
/// * `$vars` - A HashMap containing the placeholder keys and their corresponding values.
///
/// # Example
///
/// ```rust
/// use super::formatter;
/// use std::collections::HashMap;
///
/// #[test]
/// fn test_formatter() {
///     let fmt = "Hello, {name}! Your favorite color is {color}.";
///     let mut vars = HashMap::new();
///     vars.insert("name", "Alice");
///     vars.insert("color", "blue");
///
///     let result = formatter!(fmt, vars);
///
///     assert_eq!("Hello, Alice! Your favorite color is blue.", result);
/// }
/// ```
macro_rules! formatter {
    ($fmt:expr, $vars:expr) => {{
        let mut result = $fmt.to_owned();
        loop {
            if !(result.contains("{") && result.contains("}")) {
                break result;
            }
            let formatted = strfmt(&result, &$vars).unwrap_or_else(|_| result.clone());
            if formatted == result {
                break result;
            }
            result = formatted;
        }
    }};
}
