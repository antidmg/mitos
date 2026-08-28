#[macro_export]
macro_rules! hashmap {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr_2021),*) => (<[()]>::len(&[$(hashmap!(@single $rest)),*]));

    ($($key:expr_2021 => $value:expr_2021,)+) => { hashmap!($($key => $value),+) };
    ($($key:expr_2021 => $value:expr_2021),*) => {
        {
            let _cap = hashmap!(@count $($key),*);
            let mut _map = ::std::collections::HashMap::with_capacity(_cap);
            $(
                let _ = _map.insert($key, $value);
            )*
            _map
        }
    };
}
