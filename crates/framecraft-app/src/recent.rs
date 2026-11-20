// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recent projects (File menu): the last ten opened, created or saved,
//! newest first, kept in the data folder's settings. Ones that no longer
//! exist are left out.

use crate::state::Ui;
use dioxus::prelude::*;
use serde_json::Value;
use std::path::{Path, PathBuf};

const KEY: &str = "recentProjects";
const MAX: usize = 10;

pub fn load() -> Vec<PathBuf> {
    crate::layout::setting(KEY)
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_str)
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .take(MAX)
        .collect()
}

fn save(ui: Ui, list: Vec<PathBuf>) {
    crate::layout::set_setting(
        KEY,
        Value::from(
            list.iter()
                .map(|p| Value::from(p.to_string_lossy().into_owned()))
                .collect::<Vec<_>>(),
        ),
    );
    let mut recent = ui.recent;
    recent.set(list);
}

/// Puts `path` first (a project folder or a FRAME pack that exists).
pub fn add(ui: Ui, path: &Path) {
    if !path.exists() {
        return;
    }
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut list = load();
    list.retain(|p| p != &path);
    list.insert(0, path);
    list.truncate(MAX);
    save(ui, list);
}

pub fn clear(ui: Ui) {
    save(ui, Vec::new());
}

/// The menu label: the project's folder or file name, without `.framecraft`.
pub fn label(path: &Path) -> String {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    name.strip_suffix(".framecraft")
        .unwrap_or(&name)
        .to_string()
}
