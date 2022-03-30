use std::collections::HashMap;

use rusqlite::types::ToSqlOutput::{Borrowed, Owned};
use rusqlite::types::Value::{Integer, Real, Text};
use rusqlite::{ToSql};

use crate::error::Result;

/// [DynamicParam] represents a key-value pair that is going to be used in a Dynamic SQL query.
/// Typically the end user will not construct it directly but will use query object which can be
/// converted into a [Vec<DynamicParam>].
///
/// The value can be of different types so it has to be boxed. The key is `'static` because we know
/// at compile time the keys of query parameters. What we need to do at runtime is to determine which
/// keys need to be present by checking their values.
pub type DynamicParam<'p> = (&'static str, &'p dyn ToSql);

/// Same as [Display], but need a custom trait so that it can be implemented for [ToSql].
pub trait ToSqlSegment {
    fn to_sql_segment(&self) -> Result<String>;
}

impl<T: ToSql> ToSqlSegment for T {
    fn to_sql_segment(&self) -> Result<String> {
        let placeholder = "true".to_string(); // a value to indicate the existence of the parameter
        let s = match self.to_sql()? {
            Borrowed(v) => v.as_str()?.to_string(),
            Owned(v) => match v {
                Integer(i) => i.to_string(),
                Real(f) => f.to_string(),
                Text(s) => s,
                _ => placeholder,
            },
            _ => placeholder,
        };
        Ok(s)
    }
}

/// Defines behavior for a query type.
pub trait DynamicQueryParameters {
    /// Provides context for rendering SQL template. During this phase, for most parameters it is
    /// enough just to know whether values are provided or not. And if a parameter need to be substituted
    /// at this stage, the value need to be provided as [String].
    fn for_render(&self) -> HashMap<&'static str, String>;

    /// Turn parameter values into a [Vec] of trait objects. Because trait object has to be accessed through
    /// pointers and according to [trait object](https://doc.rust-lang.org/1.30.0/book/first-edition/trait-objects.html#dynamic-dispatch),
    /// only references of concrete type objects can be turned into references of trait objects.
    /// As a result, the lifetime of the resulted value is decided by the lifetime of `&self`.
    /// Note that primitive values are of owned types so they are not bound to any lifetime when
    /// the query type is constructed. Their references are created in the implementation of this
    /// function.
    fn for_execution(&self) -> Vec<DynamicParam<'_>>;
}
