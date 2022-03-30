use std::collections::HashMap;
use std::iter::FromIterator;
use std::path::Path;

use handlebars::Handlebars;
use rusqlite::{Connection, Row, ToSql};
use crate::dynamic_sql::query::DynamicQueryParameters;

use crate::dynamic_sql::template::SqlTemplate;
use crate::error::Result;

use super::sql_helpers;


/// [DynamicSqlExecutor] is the interface for performing Dynamic SQL queries. A query is dynamic if
/// the final SQL can only be determined at runtime, generated from a template based on runtime parameters.
pub trait DynamicSqlExecutor {
    /// Perform a query and return result, which is handled by `f`.
    /// Note [Into] is for `&P` instead of for `P`, see [new_query_type] for details.
    fn query<S, P, F, T>(&self, template: &S, params: P, f: F) -> Result<Vec<T>>
        where
            S: SqlTemplate,
            P: DynamicQueryParameters,
            F: FnMut(&Row<'_>) -> rusqlite::Result<T>;

    /// Execute a query and returns the number of rows that are affected.
    fn execute<S, P>(&self, template: &S, params: P) -> Result<usize>
        where
            S: SqlTemplate,
            P: DynamicQueryParameters;
}

/// Basic construct for performing Dynamic SQL queries.
/// NOTE: This struct is not [Sync] because [Connection] contains a [RefCell] and thus is not [Sync].
/// On the other hand, [Handlebars] is [Sync].
/// This indicates that it need to be wrapped inside a [Mutex] when used in a multi-threaded context.
pub struct Repository<'reg> {
    pub conn: Connection,
    handlebars: Handlebars<'reg>,
}

impl<'reg> Repository<'reg> {
    pub fn new<'a, P, T, I>(file: &P, templates: &'a T) -> Result<Self>
        where
            P: AsRef<Path> + ?Sized,
            &'a T: IntoIterator<Item = &'a I>,
            I: SqlTemplate + 'a,
    {
        let conn = Connection::open(file)?;
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

impl<'reg> DynamicSqlExecutor for Repository<'reg> {
    fn query<S, P, F, T>(&self, template: &S, params: P, f: F) -> Result<Vec<T>>
        where
            S: SqlTemplate,
            P: DynamicQueryParameters,
            F: FnMut(&Row<'_>) -> rusqlite::Result<T>,
    {
        let q = self.handlebars.render(template.name(), &params.for_render())?;
        log::debug!("{}", &q);
        let mut stmt = self.conn.prepare(&q)?;
        let result = stmt
            .query_map(params.for_execution().as_slice(), f)?
            .flat_map(|mapped_row| match mapped_row {
                Ok(inst) => Some(inst),
                Err(err) => {
                    log::warn!("failed to map row, the error is: {}", err);
                    None
                }
            });
        Ok(Vec::from_iter(result))
    }

    fn execute<S, P>(&self, template: &S, params: P) -> Result<usize>
        where
            S: SqlTemplate,
            P: DynamicQueryParameters,
    {
        let q = self.handlebars.render(template.name(), &params.for_render())?;
        log::debug!("{}", &q);
        let mut stmt = self.conn.prepare(&q)?;
        let result = stmt.execute(params.for_execution().as_slice())?;
        Ok(result)
    }
}

#[cfg(test)]
mod dog {
    use std::path::Path;

    use rusqlite::params;

    use crate::new_query_type;
    use crate::dynamic_sql::{DynamicParam, ToSqlSegment};

    use super::*;

    const DDL: &str = "CREATE TABLE IF NOT EXISTS dogs(\
                name TEXT PRIMARY KEY,\
                color TEXT,\
                weight REAL\
            );
        CREATE INDEX IF NOT EXISTS dogs_color ON dogs(color);
        CREATE INDEX IF NOT EXISTS dogs_weight ON dogs(weight);";

    pub const Q_DOGS_INSERT: (&str, &str) = (
        "Q_DOGS_INSERT",
        "INSERT INTO dogs(name, color, weight) VALUES(:name, :color, :weight)",
    );

    /// See (segment-literal notation)[https://handlebarsjs.com/guide/expressions.html#changing-the-context]
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

    pub const Q_DOGS_DELETE: (&str, &str) = ("Q_DOGS_DELETE", "DELETE FROM dogs WHERE name=?");

    pub const Q_DOGS_SELECT: (&str, &str) =
        ("Q_DOGS_SELECT", "SELECT * FROM dogs{{> Q_DOGS_WHERE }}");

    #[derive(Debug, Clone, PartialEq)]
    pub struct Dog {
        pub name: String,
        pub color: String,
        pub weight: f32,
    }

    new_query_type!(
        (DogQuery, 'q,
        p> q_name: &'q str, q_color: &'q str,
            weight_upper: f32, weight_lower: f32,)

        (DogUpdate, 'q,
        p> color: &'q str, weight: f32,
        &> query: DogQuery<'q>,)
    );

    pub struct DogStore<'reg>(Repository<'reg>);

    impl<'reg> DogStore<'reg> {
        pub(crate) fn new<P: AsRef<Path>>(db_file: &P) -> Result<Self> {
            Ok(DogStore(Repository::new(
                db_file,
                &[Q_DOGS_SELECT, Q_DOGS_UPDATE, Q_DOGS_WHERE],
            )?))
        }

        pub(crate) fn init(&mut self) -> Result<()> {
            self.0.conn.execute(DDL, [])?;
            Ok(())
        }

        pub(crate) fn add(&self, dog: Dog) -> Result<()> {
            let mut stmt = self.0.conn.prepare(Q_DOGS_INSERT.sql())?;
            stmt.execute(params!(dog.name, dog.color, dog.weight,))?;
            Ok(())
        }

        pub(crate) fn delete<T: AsRef<str>>(&self, dog_id: T) -> Result<usize> {
            let mut stmt = self.0.conn.prepare(Q_DOGS_DELETE.sql())?;
            let c = stmt.execute([dog_id.as_ref()])?;
            Ok(c)
        }

        pub(crate) fn update(&self, update: DogUpdate) -> Result<usize> {
            self.0.execute(&Q_DOGS_UPDATE, update)
        }

        pub(crate) fn list(&self, query: DogQuery) -> Result<Vec<Dog>> {
            self.0.query(&Q_DOGS_SELECT, query, |row| {
                Ok(Dog {
                    name: row.get("name").unwrap(),
                    color: row.get("color").unwrap(),
                    weight: row.get("weight").unwrap(),
                })
            })
        }
    }
}

#[cfg(test)]
mod test {
    use std::{env, fs};

    use crate::new_query_type;
    use crate::dynamic_sql::{DynamicParam, ToSqlSegment};

    use super::dog::*;
    use super::*;

    #[test]
    fn test_handlerbar() {
        let mut handlebars = Handlebars::new();
        handlebars
            .register_template_string("foo", "{{#if [:name]}}q{{/if}} {{> BAR }}")
            .unwrap();
        handlebars.register_partial("BAR", "this is bar").unwrap();
        let s = handlebars
            .render(
                "foo",
                &HashMap::<&str, &str>::from_iter(vec![(":name", "aaa"), ("value", "bbb")]),
            )
            .unwrap();
        println!("{}", s);
    }

    #[test]
    fn test_update_query_template() {
        let handlebars = get_template_engine();
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
                handlebars.render(Q_DOGS_UPDATE.name(), &update.for_render()).unwrap(),
            );
        }
    }

    #[test]
    fn test_select_query_template() {
        let handlebars = get_template_engine();
        for (params, q) in vec![
            (
                DogQuery {
                    q_name: Some("aaa"),
                    ..Default::default()
                },
                "SELECT * FROM dogs WHERE name LIKE '%' || :q_name || '%'",
            ),
            (
                DogQuery {
                    q_name: Some("aaa"),
                    q_color: Some("white"),
                    ..Default::default()
                },
                "SELECT * FROM dogs WHERE name LIKE '%' || :q_name || '%' AND color=:q_color",
            ),
            (
                DogQuery {
                    q_name: Some("aaa"),
                    q_color: Some("white"),
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
            assert_eq!(
                q,
                handlebars.render(Q_DOGS_SELECT.name(), &params.for_render()).unwrap()
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
            q_color: Some("white"),
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
        query.q_color = update.color;
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
            (FooQuery, 'q,
            p> q_name: &'q str, q_color: &'q str,
                weight_upper: f32, weight_lower: f32,)

            (FooUpdate, 'q,
            p> name: &'q str, color: &'q str,
            &> query: FooQuery<'q>,)
        );

        let q = FooQuery {
            q_name: Some("aaa"),
            ..Default::default()
        };
        assert_eq!(Some("aaa"), q.q_name);
        assert_eq!(None, q.q_color);
        assert_eq!(1, q.for_execution().len());

        let u = FooUpdate {
            name: Some("bbb"),
            query: q.clone(),
            ..Default::default()
        };
        assert_eq!(Some("bbb"), u.name);
        assert_eq!(None, u.color);
        assert_eq!(Some("aaa"), u.query.q_name);
    }

    fn get_template_engine() -> Handlebars<'static> {
        let mut handlebars = Handlebars::new();
        for t in vec![Q_DOGS_INSERT, Q_DOGS_DELETE, Q_DOGS_SELECT, Q_DOGS_UPDATE] {
            handlebars
                .register_template_string(t.name(), t.sql())
                .unwrap();
        }
        for t in vec![Q_DOGS_WHERE] {
            handlebars.register_partial(t.name(), t.sql()).unwrap();
        }
        for (name, helper) in sql_helpers() {
            handlebars.register_helper(name, helper);
        }
        handlebars
    }
}
