// SPDX-License-Identifier: AGPL-3.0-or-later
//! Template scripts (Properties → Js): named functions called from cues,
//! an `onLoad` function and globals, compiled to one classic script.

use crate::js;
use crate::model::{TemplateFunction, TemplateGlobal};

pub const ON_LOAD: &str = "onLoad";

/// A valid JavaScript identifier: other characters become `_`, and a
/// leading digit gets a `fn_` prefix.
pub fn sanitize_name(raw: &str) -> String {
    let mut out = String::new();
    for c in js::trim(raw).chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
            out.push(c);
        } else {
            // JavaScript replaces per UTF-16 unit: an emoji becomes `__`.
            out.extend(std::iter::repeat_n('_', c.len_utf16()));
        }
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert_str(0, "fn_");
    }
    out
}

fn function(id: &str, name: &str, body: &str) -> TemplateFunction {
    TemplateFunction {
        id: id.into(),
        name: name.into(),
        body: body.into(),
        extra: Default::default(),
    }
}

/// Stored functions, cleaned, with `onLoad` first.
///
/// Older graphics kept only a classic script and FRAME recovered the
/// function list from its source. FrameCraft does not do that yet: such
/// graphics keep their script as stored.
pub fn normalize_functions(
    stored: Option<&[TemplateFunction]>,
    _classic_js: &str,
) -> Vec<TemplateFunction> {
    match stored {
        Some(fns) if !fns.is_empty() => {
            let cleaned: Vec<TemplateFunction> = fns
                .iter()
                .map(|f| {
                    let name = sanitize_name(&f.name);
                    let name = if name.is_empty() {
                        "onGoal".to_string()
                    } else {
                        name
                    };
                    function(&f.id, &name, &f.body)
                })
                .collect();
            ensure_on_load(&cleaned)
        }
        _ => Vec::new(),
    }
}

/// `onLoad` first (the first non-empty body of any `onLoad`), others after.
pub fn ensure_on_load(fns: &[TemplateFunction]) -> Vec<TemplateFunction> {
    let mut others = Vec::new();
    let mut body = String::new();
    let mut id = "fn_onLoad".to_string();
    for f in fns {
        if sanitize_name(&f.name) == ON_LOAD {
            if js::trim(&body).is_empty() && !js::trim(&f.body).is_empty() {
                body = f.body.clone();
            }
            if !f.id.is_empty() {
                id = f.id.clone();
            }
        } else {
            others.push(f.clone());
        }
    }
    let mut out = vec![function(&id, ON_LOAD, &body)];
    out.extend(others);
    out
}

pub fn normalize_globals(stored: Option<&[TemplateGlobal]>) -> Vec<TemplateGlobal> {
    stored
        .unwrap_or_default()
        .iter()
        .map(|g| {
            let name = sanitize_name(&g.name);
            TemplateGlobal {
                id: g.id.clone(),
                name: if name.is_empty() {
                    "value".into()
                } else {
                    name
                },
                value_kind: match g.value_kind.as_str() {
                    "number" | "boolean" => g.value_kind.clone(),
                    _ => "string".into(),
                },
                value: g.value.clone(),
                extra: Default::default(),
            }
        })
        .collect()
}

fn global_literal(g: &TemplateGlobal, name: &str) -> String {
    let value = match g.value_kind.as_str() {
        "boolean" => {
            let v = js::trim(&g.value).to_lowercase();
            (v == "true" || v == "1" || v == "yes").to_string()
        }
        "number" => {
            let n = js::str_to_number(js::trim(&g.value));
            js::number_to_string(if n.is_finite() { n } else { 0.0 })
        }
        _ => serde_json::to_string(&g.value).expect("strings serialize"),
    };
    format!("  {name}: {value}")
}

pub fn compile_globals(globals: &[TemplateGlobal]) -> String {
    let mut used = Vec::new();
    let mut entries = Vec::new();
    for g in globals {
        let name = sanitize_name(&g.name);
        if name.is_empty() || used.contains(&name) {
            continue;
        }
        entries.push(global_literal(g, &name));
        used.push(name);
    }
    if entries.is_empty() {
        "window.__ionframeVars = {};\n".into()
    } else {
        format!("window.__ionframeVars = {{\n{}\n}};\n", entries.join(",\n"))
    }
}

/// The classic script, or an empty string when there is nothing to run.
pub fn compile(fns: &[TemplateFunction], globals: &[TemplateGlobal]) -> String {
    let list = ensure_on_load(fns);
    let mut used = Vec::new();
    let mut entries = Vec::new();
    for f in &list {
        let name = sanitize_name(&f.name);
        if name.is_empty() || used.contains(&name) {
            continue;
        }
        let body = f
            .body
            .trim_end_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
        let indented: Vec<String> = body
            .split('\n')
            .map(|l| {
                if l.is_empty() {
                    String::new()
                } else {
                    format!("    {l}")
                }
            })
            .collect();
        entries.push(format!("  {name}(ctx) {{\n{}\n  }}", indented.join("\n")));
        used.push(name);
    }

    let has_cue_fns = list.iter().any(|f| {
        let n = sanitize_name(&f.name);
        !n.is_empty() && n != ON_LOAD
    });
    let on_load_body = list
        .iter()
        .find(|f| sanitize_name(&f.name) == ON_LOAD)
        .map_or("", |f| js::trim(&f.body));
    let has_globals = globals.iter().any(|g| !sanitize_name(&g.name).is_empty());
    if !has_cue_fns && on_load_body.is_empty() && !has_globals {
        return String::new();
    }

    format!(
        "// Generated from Script editor — Properties → Js.\n\
         // Globals: window.__ionframeVars  |  Functions: window.__ionframeScripts\n\
         {}window.__ionframeScripts = {{\n{}\n}};\n",
        compile_globals(globals),
        entries.join(",\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_name(" on goal "), "on_goal");
        assert_eq!(sanitize_name("2nd"), "fn_2nd");
        assert_eq!(sanitize_name("mää"), "m__");
        assert_eq!(sanitize_name("a😀"), "a__");
    }

    #[test]
    fn empty_script_compiles_to_nothing() {
        assert_eq!(compile(&[], &[]), "");
    }

    #[test]
    fn compiles_functions_and_globals() {
        let fns = [
            function("a", "onGoal", "log(1);\n\nlog(2);  \n"),
            function("b", "onLoad", "init();"),
        ];
        let globals = [TemplateGlobal {
            id: "g".into(),
            name: "count".into(),
            value_kind: "number".into(),
            value: " 2.50 ".into(),
            extra: Default::default(),
        }];
        assert_eq!(
            compile(&fns, &globals),
            "// Generated from Script editor — Properties → Js.\n\
             // Globals: window.__ionframeVars  |  Functions: window.__ionframeScripts\n\
             window.__ionframeVars = {\n  count: 2.5\n};\n\
             window.__ionframeScripts = {\n  onLoad(ctx) {\n    init();\n  },\n  onGoal(ctx) {\n    log(1);\n\n    log(2);\n  }\n};\n"
        );
    }
}
