use handlebars::{
    Context, Handlebars, Helper, HelperDef, HelperResult, Output, RenderContext, RenderError,
    Renderable,
};

pub fn sql_helpers() -> Vec<(&'static str, Box<dyn HelperDef + Send + Sync>)> {
    return vec![
        ("set", Box::new(set_block)),
        ("where", Box::new(where_block)),
        ("trim", Box::new(trim_block)),
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
