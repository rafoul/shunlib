use std::collections::HashMap;
use std::iter::FromIterator;

use handlebars::Handlebars;
use rusqlite::{Connection, Row, ToSql};
use serde::{Serialize, Deserialize};

use crate::error::Result;

use super::sql_helpers;

pub trait SqlTemplate {
    fn name(&self) -> &str;

    fn sql(&self) -> &str;
}

/// Trait object is necessary because the values of parameters can be of different types.
pub type DynamicParam<'k, 'v> = (&'k str, &'v dyn ToSql);

pub trait RenderSql {
    /// Note the item of the iterator is `&'p DynamicParam<'k, 'v>`, this is why `'k` and `'v`` must
    /// lasts longer than `'p`. Because reference cannot have a longer lifetime than the data it references.
    /// This indicates that `'k` and `'v` exists before `'p`, so they should be input parameters as well.
    fn render_dynamic_sql<'k: 'p, 'v: 'p, 'p, T, P>(&self, t: &T, params: &'p P) -> Result<String>
    where
        T: SqlTemplate,
        &'p P: IntoIterator<Item = &'p DynamicParam<'k, 'v>>;
}

pub trait DynamicSqlExecutor {
    /// Note how the lifetime requirement is different from `render_dynamic_sql`.
    /// The `params` here is a structure holding user inputs.
    fn query<'p, 'k, S, P, F, T>(&self, template: &S, params: &'p P, f: F) -> Result<Vec<T>>
    where
        S: SqlTemplate,
        &'p P: Into<Vec<DynamicParam<'k, 'p>>>,
        F: FnMut(&Row<'_>) -> rusqlite::Result<T>;

    fn execute<'p, 'k, S, P>(&self, template: &S, params: &'p P) -> Result<usize>
    where
        S: SqlTemplate,
        &'p P: Into<Vec<DynamicParam<'k, 'p>>>;
}

pub struct Repository<'reg> {
    pub conn: Connection,
    handlebars: Handlebars<'reg>,
}

/// The macro is need because first of all, the value can be of different types, so they cannot be
/// put into a `Vec` directly. And we cannot treat the value as `Option` once it becomes a `dyn` ref.
/// So the filtering can only be done for it is placed into a `Vec`.
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

impl<'reg> RenderSql for Handlebars<'reg> {
    fn render_dynamic_sql<'k: 'p, 'v: 'p, 'p, T, P>(&self, t: &T, params: &'p P) -> Result<String>
    where
        T: SqlTemplate,
        &'p P: IntoIterator<Item = &'p DynamicParam<'k, 'v>>,
    {
        let ctx = HashMap::<&str, bool>::from_iter(params.into_iter().map(|(k, _)| (*k, true)));
        Ok(self.render(t.name(), &ctx)?)
    }
}

impl<'reg> Repository<'reg> {
    pub fn new<'a, T, I>(conn: Connection, templates: &'a T) -> Result<Self>
    where
        &'a T: IntoIterator<Item = &'a I>,
        I: SqlTemplate + 'a,
    {
        let mut handlebars = Handlebars::new();
        for q in templates {
            handlebars.register_template_string(q.name(), q.sql())?;
        }

        for (k, h) in sql_helpers() {
            handlebars.register_helper(k, h);
        }
        Ok(Repository { conn, handlebars })
    }
}

impl SqlTemplate for (&str, &str) {
    fn name(&self) -> &str {
        self.0
    }

    fn sql(&self) -> &str {
        self.1
    }
}

impl<'reg> DynamicSqlExecutor for Repository<'reg> {
    fn query<'p, 'k, S, P, F, T>(&self, template: &S, params: &'p P, f: F) -> Result<Vec<T>>
    where
        S: SqlTemplate,
        &'p P: Into<Vec<DynamicParam<'k, 'p>>>,
        F: FnMut(&Row<'_>) -> rusqlite::Result<T>,
    {
        let params = params.into();
        let q = self.handlebars.render_dynamic_sql(template, &params)?;
        let mut stmt = self.conn.prepare(&q)?;
        let result = stmt
            .query_map(params.as_slice() as &[(&str, &dyn ToSql)], f)?
            .flat_map(|mapped_row| match mapped_row {
                Ok(inst) => Some(inst),
                Err(err) => {
                    log::warn!("failed to map row, the error is: {}", err);
                    None
                }
            });
        Ok(Vec::from_iter(result))
    }

    fn execute<'p, 'k, S, P>(&self, template: &S, params: &'p P) -> Result<usize>
    where
        S: SqlTemplate,
        &'p P: Into<Vec<DynamicParam<'k, 'p>>>,
    {
        let params = params.into();
        let q = self.handlebars.render_dynamic_sql(template, &params)?;
        let mut stmt = self.conn.prepare(&q)?;
        let result = stmt.execute(params.as_slice() as &[(&str, &dyn ToSql)])?;
        Ok(result)
    }
}

/// A DSL to define structs for holding parameters of dynamic queries. It is assumed that there
/// are two types of queries, i.e. dynamic select and dynamic update. Because the fields appears
/// in the `SET` clause can conflict with those in the `WHERE` clause, the parameters in the latter
/// will thus be named differently, with a `':q_'` prefix. And an additional section is used to
/// reuse the struct for dynamic select in the dynamic update struct.
/// For cases not covered by this macro, one can also manually create a struct and implement the traits
/// accordingly.
#[macro_export]
macro_rules! new_query_type {
    ( $s:ident, $l:lifetime,
        $( -> $($f:ident: $t:ty,)* )?
        $( => $($f1:ident: $t1:ty,)* )?
        $( &> $($r:ident: $rt:ty,)* )?
    ) => {
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
    }
}

#[cfg(test)]
mod dog {
    use std::path::Path;

    use lazy_static::lazy_static;
    use rusqlite::params;

    use super::*;

    lazy_static! {
        static ref DDL: Vec<&'static str> = vec![
            "CREATE TABLE IF NOT EXISTS dogs(\
                name TEXT PRIMARY KEY,\
                color TEXT,
                weight REAL
            )",
            "CREATE INDEX IF NOT EXISTS dogs_color ON dogs(color)",
            "CREATE INDEX IF NOT EXISTS dogs_weight ON dogs(weight)",
        ];
    }

    pub const Q_DOGS_INSERT: &str =
        "INSERT INTO dogs(name, color, weight) VALUES(:name, :color, :weight)";

    pub const Q_DOGS_UPDATE: (&str, &str) = (
        "Q_DOGS_UPDATE",
        "UPDATE dogs{{#set}}\
        {{#if [:color]}}color=:color, {{/if}}\
        {{#if [:weight]}}weight=:weight, {{/if}}{{/set}}\
    {{> Q_DOGS_WHERE }}",
    );

    pub const Q_DOGS_WHERE: (&str, &str) = (
        "Q_DOGS_WHERE",
        "{{#where}}\
        {{#if [:q_name]}} AND name LIKE '%' || :q_name || '%'{{/if}}\
        {{#if [:q_color]}} AND color=:q_color{{/if}}\
        {{#if [:weight_upper]}} AND weight<=:weight_upper{{/if}}\
        {{#if [:weight_lower]}} AND weight>=:weight_lower{{/if}}\
        {{/where}}",
    );

    pub const Q_DOGS_DELETE: &str = "DELETE FROM dogs WHERE name=?";

    pub const Q_DOGS_SELECT: (&str, &str) =
        ("Q_DOGS_SELECT", "SELECT * FROM dogs{{> Q_DOGS_WHERE }}");

    #[derive(Debug, Clone, PartialEq)]
    pub struct Dog {
        pub name: String,
        pub color: String,
        pub weight: f32,
    }

    new_query_type!(
        DogQuery, 'q,
        -> name: &'q str, color: &'q str,
        => weight_upper: f32, weight_lower: f32,
    );

    new_query_type!(
        DogUpdate, 'q,
        => color: &'q str, weight: f32,
        &> query: DogQuery<'q>,
    );

    pub struct DogStore<'reg>(Repository<'reg>);

    impl<'reg> DogStore<'reg> {
        pub(crate) fn new<P: AsRef<Path>>(db_file: P) -> Result<Self> {
            Ok(DogStore(Repository::new(
                Connection::open(db_file)?,
                &[Q_DOGS_SELECT, Q_DOGS_UPDATE, Q_DOGS_WHERE],
            )?))
        }

        pub(crate) fn init(&mut self) -> Result<()> {
            for q in DDL.iter() {
                self.0.conn.execute(q, [])?;
            }
            Ok(())
        }

        pub(crate) fn add(&self, dog: Dog) -> Result<()> {
            let mut stmt = self.0.conn.prepare(Q_DOGS_INSERT)?;
            stmt.execute(params!(dog.name, dog.color, dog.weight,))?;
            Ok(())
        }

        pub(crate) fn delete<T: AsRef<str>>(&self, dog_id: T) -> Result<usize> {
            let mut stmt = self.0.conn.prepare(Q_DOGS_DELETE)?;
            let c = stmt.execute([dog_id.as_ref()])?;
            Ok(c)
        }

        pub(crate) fn update(&self, update: DogUpdate) -> Result<usize> {
            self.0.execute(&Q_DOGS_UPDATE, &update)
        }

        pub(crate) fn list(&self, query: DogQuery) -> Result<Vec<Dog>> {
            self.0.query(&Q_DOGS_SELECT, &query, |row| {
                Ok(Dog {
                    name: row.get("name").unwrap(),
                    color: row.get("color").unwrap(),
                    weight: row.get("weight").unwrap(),
                })
            })
        }
    }
}

pub fn prepare_template_engine<T: IntoIterator<Item = I>, I: SqlTemplate>(
    t: I,
    partials: T,
) -> Handlebars<'static> {
    let mut handlebars = Handlebars::new();
    handlebars
        .register_template_string(t.name(), t.sql())
        .unwrap();
    for t in partials {
        handlebars.register_partial(t.name(), t.sql()).unwrap();
    }
    for (name, helper) in sql_helpers() {
        handlebars.register_helper(name, helper);
    }
    handlebars
}

#[cfg(test)]
mod test {
    use std::{env, fs};

    use super::dog::*;
    use super::*;

    #[test]
    fn test_handlerbar() {
        let mut handlebars = Handlebars::new();
        handlebars
            .register_template_string("foo", "aaa {{> BAR }}")
            .unwrap();
        handlebars.register_partial("BAR", "this is bar").unwrap();
        let s = handlebars
            .render(
                "foo",
                &HashMap::<&str, &str>::from_iter(vec![("name", "aaa"), ("value", "bbb")]),
            )
            .unwrap();
        println!("{}", s);
    }

    #[test]
    fn test_update_query_template() {
        let handlebars = prepare_template_engine(Q_DOGS_UPDATE, vec![Q_DOGS_WHERE]);
        for (update, q) in vec![
            (
                DogUpdate {
                    color: Some("white"),
                    weight: Some(50.5),
                    ..Default::default()
                },
                "UPDATE dogs SET color=:color, weight=:weight",
            ),
            (
                DogUpdate {
                    color: Some("white"),
                    ..Default::default()
                },
                "UPDATE dogs SET color=:color",
            ),
        ]
        .into_iter()
        {
            assert_eq!(
                q,
                handlebars
                    .render_dynamic_sql(&Q_DOGS_UPDATE, &Into::<Vec<DynamicParam>>::into(&update))
                    .unwrap()
            );
        }
    }

    #[test]
    fn test_select_query_template() {
        let handlebars = prepare_template_engine(Q_DOGS_SELECT, vec![Q_DOGS_WHERE]);
        for (params, q) in vec![
            (
                DogQuery {
                    name: Some("aaa"),
                    ..Default::default()
                },
                "SELECT * FROM dogs WHERE name LIKE '%' || :q_name || '%'",
            ),
            (
                DogQuery {
                    name: Some("aaa"),
                    color: Some("white"),
                    ..Default::default()
                },
                "SELECT * FROM dogs WHERE name LIKE '%' || :q_name || '%' AND color=:q_color",
            ),
            (
                DogQuery {
                    name: Some("aaa"),
                    color: Some("white"),
                    weight_upper: Some(50.5),
                    weight_lower: Some(10.5),
                },
                "SELECT * FROM dogs WHERE name LIKE '%' || :q_name || '%' AND color=:q_color \
                AND weight<=:weight_upper AND weight>=:weight_lower",
            ),
            (
                DogQuery {
                    ..Default::default()
                },
                "SELECT * FROM dogs",
            ),
        ] {
            let params = Into::<Vec<DynamicParam>>::into(&params);
            assert_eq!(
                q,
                handlebars
                    .render_dynamic_sql(&Q_DOGS_SELECT, &params)
                    .unwrap()
            )
        }
    }

    #[test]
    fn test_movie_store() {
        let file = env::temp_dir().join("dog_store_test");
        if file.exists() {
            fs::remove_file(&file).unwrap();
        }
        let mut store = DogStore::new(&file).unwrap();
        store.init().unwrap();

        let dog = Dog {
            name: "Jeff".to_string(),
            color: "white".to_string(),
            weight: 20.5,
        };
        store.add(dog.clone()).unwrap();

        let mut query = DogQuery {
            color: Some("white"),
            ..Default::default()
        };
        let query_fn = |q: DogQuery| store.list(q).unwrap();
        let query_result = query_fn(query.clone());
        assert_eq!(&dog, &query_result[0]);

        let update = DogUpdate {
            color: Some("yellow"),
            weight: Some(30.2),
            query: query.clone(),
        };
        store.update(update.clone()).unwrap();
        query.color = update.color;
        let query_result = query_fn(query.clone());
        let updated = &query_result[0];
        assert_eq!(update.color.as_ref().unwrap(), &updated.color,);
        assert_eq!(update.weight.unwrap(), updated.weight);

        store.delete(&dog.name).unwrap();
        let query_result = query_fn(query);
        assert!(query_result.is_empty());
    }

    #[test]
    fn test_new_query_type() {
        new_query_type!(
            FooQuery, 'q,
            -> name: &'q str, color: &'q str,
            => weight_upper: f32, weight_lower: f32,
        );

        new_query_type!(
            FooUpdate, 'q,
            => name: &'q str, color: &'q str,
            &> query: FooQuery<'q>,
        );

        let q = FooQuery {
            name: Some("aaa"),
            ..Default::default()
        };
        assert_eq!(Some("aaa"), q.name);
        assert_eq!(None, q.color);
        assert_eq!(1, Vec::<(&str, &dyn ToSql)>::from(&q).len());

        let u = FooUpdate {
            name: Some("bbb"),
            query: q.clone(),
            ..Default::default()
        };
        assert_eq!(Some("bbb"), u.name);
        assert_eq!(None, u.color);
        assert_eq!(Some("aaa"), u.query.name);
    }
}
