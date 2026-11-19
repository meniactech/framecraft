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

// ---------------------------------------------------------------------------
// Editing (FRAME's script editor)

/// FRAME's default body for a new function.
pub fn starter_body(name: &str) -> String {
    if name == ON_LOAD {
        "// Runs when the template loads (editor + SPX/export).\n// ctx: { reason }\n// Globals: window.__ionframeVars\n".into()
    } else {
        format!(
            "// ctx: {{ cueId, label }}\n// Globals: window.__ionframeVars\nconsole.log('[template.js] {name}', ctx);"
        )
    }
}

/// A new function (FRAME's `createTemplateFunction`).
pub fn new_function(name: &str, body: Option<&str>) -> TemplateFunction {
    let name = match sanitize_name(name) {
        n if n.is_empty() => "onGoal".to_string(),
        n => n,
    };
    let id = if name == ON_LOAD {
        "fn_onLoad".to_string()
    } else {
        crate::layers::new_id()
    };
    let body = body.map_or_else(|| starter_body(&name), str::to_string);
    function(&id, &name, &body)
}

/// FRAME's Starter: `onLoad` and an `onGoal` cue function.
pub fn starter_functions() -> Vec<TemplateFunction> {
    vec![new_function(ON_LOAD, None), new_function("onGoal", None)]
}

/// The first free `<prefix>N` name (N from `len + 1`), never `onLoad`.
pub fn next_name(prefix: &str, taken: &[String]) -> String {
    let mut n = taken.len() + 1;
    loop {
        let name = format!("{prefix}{n}");
        if !taken.iter().any(|t| sanitize_name(t) == name) && name != ON_LOAD {
            return name;
        }
        n += 1;
    }
}

/// A new global (FRAME's Add global: `varN`, a number 0).
pub fn new_global(taken: &[String]) -> TemplateGlobal {
    TemplateGlobal {
        id: crate::layers::new_id(),
        name: next_name("var", taken),
        value_kind: "number".into(),
        value: "0".into(),
        extra: Default::default(),
    }
}

/// Functions a Script cue can call (all but `onLoad`).
pub fn cue_function_names(fns: &[TemplateFunction]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in fns {
        let n = sanitize_name(&f.name);
        if !n.is_empty() && n != ON_LOAD && !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// Replaces the graphic's script functions and globals (names cleaned,
/// `onLoad` kept first and unrenamed) and recompiles the shipped script.
pub struct SetScript {
    pub functions: Vec<TemplateFunction>,
    pub globals: Vec<TemplateGlobal>,
}

impl crate::commands::Command for SetScript {
    fn apply(
        &self,
        doc: &mut crate::normalize::Document,
    ) -> Result<crate::commands::Change, crate::commands::CommandError> {
        let functions = normalize_functions(Some(&self.functions), "");
        let functions = if functions.is_empty() {
            ensure_on_load(&[])
        } else {
            functions
        };
        let globals = normalize_globals(Some(&self.globals));
        doc.template_js = compile(&functions, &globals);
        doc.template_functions = functions;
        doc.template_globals = globals;
        Ok(crate::commands::Change::Rebuild)
    }
    fn label(&self, _doc: &crate::normalize::Document) -> String {
        "Edit script".into()
    }
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

    #[test]
    fn editing_keeps_on_load_first_and_recompiles() {
        use crate::commands::History;
        let g = serde_json::from_value(serde_json::json!({
            "scene": { "id": "scene", "name": "S", "backgroundColor": "#000", "width": 1920, "height": 1080 },
            "items": []
        }))
        .unwrap();
        let mut d = crate::normalize::normalize(&g, &crate::profile::Profile::framecraft());
        let mut fns = starter_functions();
        assert_eq!(
            next_name(
                "fn",
                &fns.iter().map(|f| f.name.clone()).collect::<Vec<_>>()
            ),
            "fn3"
        );
        fns.push(new_function("2 goals!", Some("return 2;")));
        let globals = vec![new_global(&[])];
        let mut h = History::default();
        h.execute(
            &mut d,
            &SetScript {
                functions: fns.into_iter().rev().collect(),
                globals,
            },
        )
        .unwrap();
        assert_eq!(d.template_functions[0].name, ON_LOAD);
        assert_eq!(
            cue_function_names(&d.template_functions),
            ["fn_2_goals_", "onGoal"]
        );
        assert!(d.template_js.contains("fn_2_goals_(ctx)") && d.template_js.contains("var1: 0"));
        h.undo(&mut d).unwrap();
        assert!(!d.template_js.contains("fn_2_goals_"));
    }
}
