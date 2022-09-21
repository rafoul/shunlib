use handlebars::{
    Context, Handlebars, Helper, HelperDef, HelperResult, Output, RenderContext, RenderError,
    Renderable,
};
use itertools::Itertools;

pub fn sql_helpers() -> Vec<(&'static str, Box<dyn HelperDef + Send + Sync>)> {
    return vec![
        ("set", Box::new(set_block)),
        ("where", Box::new(where_block)),
        ("trim", Box::new(trim_block)),
        ("in", Box::new(in_block))
    ];
}

/// Help to avoid duplicating function declarations.
macro_rules! define_trim_block {
    ( $( ($name:ident, $prefix:expr, $token:expr), )+ )=> {
        $(
            fn $name<'reg, 'rc>(
                h: &Helper<'reg, 'rc>,
                r: &'reg Handlebars<'reg>,
                ctx: &'rc Context,
                rc: &mut RenderContext<'reg, 'rc>,
                out: &mut dyn Output,
            ) -> HelperResult {
                trim_block_helper(h, r, ctx, rc, out, $prefix, $token)
            }
        )+
    };
}

define_trim_block!(
    (trim_block, "", None),
    (where_block, "WHERE", Some("AND ")),
    (set_block, "SET", Some(",")),
);

fn trim_block_helper<'reg, 'rc>(
    h: &Helper<'reg, 'rc>,
    r: &'reg Handlebars<'reg>,
    ctx: &'rc Context,
    rc: &mut RenderContext<'reg, 'rc>,
    out: &mut dyn Output,
    prefix: &str,
    token: Option<&str>,
) -> HelperResult {
    if let Some(t) = h.template() {
        let content = t.renders(r, ctx, rc)?;
        let token = token
            .or_else(|| h.param(0).and_then(|v| v.value().as_str()))
            .ok_or(RenderError::new(
                "delimiter is required for trimming helpers",
            ))
            .unwrap();
        let mut content = content.trim();
        if !content.is_empty() {
            content = content.trim_start_matches(token);
            content = content.trim_end_matches(token);
            for s in &[" ", prefix, " ", content] {
                out.write(s)?;
            }
        }
    }
    Ok(())
}

fn in_block<'reg, 'rc>(
    h: &Helper<'reg, 'rc>,
    r: &'reg Handlebars<'reg>,
    ctx: &'rc Context,
    rc: &mut RenderContext<'reg, 'rc>,
    out: &mut dyn Output,
) -> HelperResult {
    let values = h.param(0).ok_or(RenderError::new("values must be provided for `IN` block"))?
        .value()
        .as_str()
        .ok_or(RenderError::new("values must be provided as a valid string"))?
        .split(',')
        .unique()
        .map(|it| format!("'{}'", it))
        .collect::<Vec<String>>();
    let replacement = values.join(",");
    let mut inner_content = h.template().ok_or(RenderError::new("content cannot be empty for `IN` block"))?
        .renders(r, ctx, rc)?;
    for placeholder in vec![":VALUES", ":values"] {
        inner_content = inner_content.replace(placeholder, &replacement);
    }
    out.write(&inner_content)?;
    Ok(())
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;
    use std::iter::FromIterator;
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
    fn test_trim_block() {
        let mut handlebars = Handlebars::new();
        handlebars
            .register_helper("where", Box::new(where_block));
        handlebars
            .register_template_string("foo", r#"{{#where}}AND a=:a AND b=:b{{/where}}"#)
            .unwrap();
        let result = handlebars.render(
            "foo",
            &1,
        ).unwrap();
        assert_eq!("WHERE a=:a AND b=:b", result.trim());
    }

    #[test]
    fn test_in_block_helper() {
        let mut handlebars = Handlebars::new();
        handlebars
            .register_helper("in", Box::new(in_block));
        handlebars
            .register_template_string("foo", r#"{{#in "a,b,c"}}IN (:VALUES){{/in}}"#)
            .unwrap();
        let result = handlebars.render(
            "foo",
            &1,
        ).unwrap();
        assert_eq!("IN ('a','b','c')", result.as_str());
    }
}