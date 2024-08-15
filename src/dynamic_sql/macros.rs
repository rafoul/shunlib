/// Cast values to `[dyn ToSql]`. Note that the value has to be a reference for it to be cast
/// into a trait object.
#[macro_export]
macro_rules! build_dynamic_params {
    ( $( $key:expr, $value:expr, )* ) => {
        {
            let mut v = Vec::<(&str, &dyn ToSql)>::new();
            $(
                    if $value.is_some() {
                        v.push(($key, &$value as &dyn rusqlite::ToSql));
                    }
            )*
            v
        }
    }
}

/// Macro for defining query types. Query types is used for collecting parameter values which are
/// used for SQL statements and can only be known at runtime.   There are two phases for processing
/// dynamic queries:
/// 1. Rendering the SQL template into string. Some parameters are replaced by their values at this
/// phase, e.g. `limit`.
/// 2. Execute the SQL statement and provide values for bind parameters.
///
/// The lifetime for the query type is optional. Sometimes it might be convenient to provide the input
/// as references. But this is not always necessary because primitive types are usually not borrowed.
///
/// The syntax is as below:
/// `->`: parameters used in phase 2 as mentioned above.
/// `=>`: parameters used in phase 1 as mentioned above
/// `&>`: fields that reference other query types. Fields in referenced types are treated as if they
/// are defined as part of the referencing type. Please note that fields should be named differently
/// if they happen to have the same name in referenced types and the referencing type. For example,
/// if `FooUpdate` reference `FooQuery` and `name` appears in both, then one should named like `q_name`
/// while the other is `name`.
#[macro_export]
macro_rules! new_query_type {
    (
        $(
            (
                $s:ident, $( $l:lifetime, )?
                $( -> $($pf:ident: $pt:ty,)* )?
                $( => $($cf:ident: $ct:ty,)* )?
                $( &> $($r:ident: $rt:ty,)* )?
            )
        )+
    ) => {
        use serde::{Deserialize, Serialize};
        use $crate::build_dynamic_params;
        use std::collections::HashMap;

        $(
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct $s$(<$l>)? {
            $( $( pub $pf: Option<$pt>, )* )?
            $( $( pub $cf: Option<$ct>, )* )?
            $(
                $(
                    #[serde(flatten)]
                    pub $r: Option<$rt>,
                )*
            )?
        }

        impl$(<$l>)? Default for $s$(<$l>)? {
            fn default() -> Self {
                $s {
                    $( $( $pf: None, )* )?
                    $( $( $cf: None, )* )?
                    $( $( $r: None, )* )?
                }
            }
        }

        impl$(<$l>)? DynamicQueryParameters for $s$(<$l>)? {
            fn for_render(&self) -> HashMap<&'static str, String> {
                let v = build_dynamic_params!(
                    $( $( concat!(":", stringify!($pf)), self.$pf, )* )?
                    $( $( concat!(":", stringify!($cf)), self.$cf, )* )?
                );
                let v = HashMap::<&'static str, String>::from_iter(
                    v.into_iter().map(|(k, v)| (k, v.to_sql_segment().unwrap_or("".to_string()))),
                );
                $(
                    $(
                        let v = if let Some(ref $r) = self.$r {
                            let mut v = v;
                            v.extend($r.for_render());
                            v
                        } else {
                            v
                        };
                    )*
                )?
                v
            }

            fn for_execution(&self) -> Vec<DynamicParam<'_>> {
                 let v = build_dynamic_params!(
                    $( $( concat!(":", stringify!($pf)), self.$pf, )* )?
                );
                $(
                    $(
                        let v = if let Some(ref $r) = self.$r {
                            let mut v = v;
                            v.append(&mut $r.for_execution());
                            v
                        } else {
                            v
                        };
                    )*
                )?
                v
            }
        }

        )+
    }
}
