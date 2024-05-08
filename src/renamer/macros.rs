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
        $ev.$x(move |_| _ = this.rename_workspace());
        )*
    };
}
