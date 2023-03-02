#[macro_export]
macro_rules! uppercase_keys_of {
    ( $config:ident, $( $x:ident ), * ) => {
        $(
        let $x = $config
            .$x
            .iter()
            .map(|(k, v)| (k.to_uppercase(), v.clone()))
            .collect::<FxHashMap<_, _>>();
        )*
    };
}
