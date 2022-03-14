#[macro_export]
macro_rules! build_dynamic_params {
    ( $( ($key:expr, $value:expr), )+ ) => {
        {
            let mut v = Vec::<(&str, &dyn ToSql)>::new();
            $(
                    if $value.is_some() {
                        v.push(($key, &$value as &dyn rusqlite::ToSql));
                    }
            )+
            v
        }
    }
}

/// Macro for defining query types. Query types serve two purposes. On one hand they are used for
/// collecting user inputs. On the other hand, they can be turned into parameters that can be directly
/// used for executing SQL queries.
/// Typically there are two locations parameters can appear in a SQL statement. One is for writing
/// values, e.g. in `SET` or `INSERT`. The other is in `WHERE` clause. It is possible that one field
/// in a query type appears at both locations in one SQL statement, typically in an `UPDATE` statement that
/// contains `WHERE` clause. Under such a case, it is desired to make them as two separate parameters.
///
/// The syntax is as below:
/// `->`: fields that appears in both queries and updates, the param used in queries will be prefixed
/// with `':q_'`.
/// `=>`: fields that appears in either queries or updates but not both.
/// `&>`: fields that reference other query types. Fields in referenced types are treated as if they
/// were defined as part of the referencing type.
#[macro_export]
macro_rules! new_query_type {
    (
        $(
            (
                $s:ident, $l:lifetime,
                $( -> $($f:ident: $t:ty,)* )?
                $( => $($f1:ident: $t1:ty,)* )?
                $( &> $($r:ident: $rt:ty,)* )?
            )
        )+
    ) => {
        use serde::{Deserialize, Serialize};
        use $crate::build_dynamic_params;

        $(
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct $s<$l> {
            $( $( pub $f: Option<$t>, )* )?
            $( $( pub $f1: Option<$t1>, )* )?
            $( $( pub $r: $rt, )* )?
        }

        impl Default for $s<'_> {
            fn default() -> Self {
                $s {
                    $( $( $f: None, )* )?
                    $( $( $f1: None, )* )?
                    $( $( $r: Default::default(), )* )?
                }
            }
        }

        impl<'a, 'q> From<&'a $s<'q>> for Vec<(&str, &'a dyn ToSql)> {
            #[warn(unused_mut)]
            fn from(q: &'a $s<'q>) -> Self {
                 let v = build_dynamic_params!(
                    $( $( (concat!(":q_", stringify!($f)), q.$f), )* )?
                    $( $( (concat!(":", stringify!($f1)), q.$f1), )* )?
                 );
                 $(
                    let mut v = v;
                    $( v.append(&mut (&q.$r).into()); )*
                 )?
                 v
            }
        }
        )+
    }
}
