#[macro_export]
macro_rules! rename_workspace_if {
    ( $self: ident, $ev: ident, $( $x:ident ), * ) => {
        $(
        let this = $self.clone();
        $ev.$x(move |_, _| _ = this.renameworkspace());
        )*
    };
}
