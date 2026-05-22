pub fn gen_error_enum(name: &str, variants: &[(String, Option<String>)]) -> String {
    let mut out = String::new();

    out.push_str(&format!("#[derive(Debug, Clone, thiserror::Error)]\n"));
    out.push_str(&format!("pub enum {name} {{\n"));

    for (variant, payload) in variants {
        match payload {
            Some(ty) => {
                out.push_str(&format!(
                    "    #[error(\"{variant}: {{0}}\")]\n    {variant}({ty}),\n"
                ));
            }
            None => {
                out.push_str(&format!(
                    "    #[error(\"{variant}\")]\n    {variant},\n"
                ));
            }
        }
    }

    out.push_str("}\n\n");

    // impl From<ViraError>
    out.push_str(&format!(
        "impl From<ViraError> for {name} {{\n    fn from(e: ViraError) -> Self {{\n        {name}::__Vira(e.message)\n    }}\n}}\n\n"
    ));

    out
}

/// Transform a return type: T! → ViraResult<T>, T!E → Result<T, E>
pub fn transform_result_type(ok_type: &str, err_type: Option<&str>) -> String {
    match err_type {
        Some(e) => format!("Result<{ok_type}, {e}>"),
        None    => format!("ViraResult<{ok_type}>"),
    }
}

/// Transform `throw expr` → `return Err(ViraError::new(expr_str).into())`
pub fn gen_throw(expr_str: &str) -> String {
    // If expr looks like ErrorType::Variant(...) — use directly
    if expr_str.contains("::") {
        format!("return Err(({expr_str}).into())")
    } else {
        format!("return Err(ViraError::new(format!(\"{{:?}}\", {expr_str})).into())")
    }
}

/// Transform `try { body } catch |e| { handler }` → match
pub fn gen_try_catch(body_str: &str, err_binding: &str, handler_str: &str) -> String {
    format!(
        "(|| -> ViraResult<_> {{\n{body_str}\n}})().unwrap_or_else(|{err_binding}| {{\n{handler_str}\n}})"
    )
}

/// Emit the thiserror dependency for Cargo.toml
pub fn thiserror_dep() -> &'static str {
    "thiserror = \"1\""
}
