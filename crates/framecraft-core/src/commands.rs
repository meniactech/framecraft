// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every edit to a graphic is a [`Command`]. Commands report how much of the
//! preview must update ([`Change`]); [`History`] makes them undoable.
//!
//! Undo keeps a snapshot of the document before each step. Graphics are
//! small (tens to hundreds of KB), so this is cheap and always correct.
//! A gesture (a drag in the viewport, scrubbing a number) groups many
//! commands into one undo step.

use crate::model::{Item, ItemTrack, Keyframe};
use crate::normalize::Document;
use crate::timeline::{base_value, lane_keys, value_at};
use std::collections::BTreeMap;

/// How much of the preview a change affects (cheapest first).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Nothing visible changed (a rename, a lock).
    None,
    /// Animated values only: patch the runtime's data and repaint.
    Pose,
    /// These layers' markup changed: swap them in place.
    Layer(Vec<String>),
    /// Layers were added, removed or restructured: rebuild the preview.
    Rebuild,
}

impl Change {
    /// The update covering both changes.
    pub fn merge(self, other: Change) -> Change {
        use Change::*;
        match (self, other) {
            (Rebuild, _) | (_, Rebuild) => Rebuild,
            (Layer(mut a), Layer(b)) => {
                for id in b {
                    if !a.contains(&id) {
                        a.push(id);
                    }
                }
                Layer(a)
            }
            (Layer(a), _) | (_, Layer(a)) => Layer(a),
            (Pose, _) | (_, Pose) => Pose,
            (None, None) => None,
        }
    }
}

/// Why a command could not run. The document is unchanged when it fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    NoSuchItem(String),
    NoSuchKeyframe {
        item: String,
        lane: String,
        time_ms: String,
    },
    UnknownField(String),
    WouldCreateCycle(String),
    InvalidValue(String),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::NoSuchItem(id) => write!(f, "no layer with id {id}"),
            Self::NoSuchKeyframe {
                item,
                lane,
                time_ms,
            } => write!(f, "{item} has no {lane} keyframe at {time_ms} ms"),
            Self::UnknownField(name) => write!(f, "unknown field {name}"),
            Self::WouldCreateCycle(id) => write!(f, "{id} cannot be moved inside itself"),
            Self::InvalidValue(msg) => write!(f, "invalid value: {msg}"),
        }
    }
}

impl std::error::Error for CommandError {}

pub trait Command {
    /// Changes the document. On error the document must be unchanged.
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError>;
    /// Shown in Edit → Undo ("Move Lower third").
    fn label(&self, doc: &Document) -> String;
}

pub(crate) fn item_mut<'a>(doc: &'a mut Document, id: &str) -> Result<&'a mut Item, CommandError> {
    doc.items
        .iter_mut()
        .find(|i| i.id == id)
        .ok_or_else(|| CommandError::NoSuchItem(id.to_string()))
}

pub(crate) fn item_name(doc: &Document, id: &str) -> String {
    doc.items
        .iter()
        .find(|i| i.id == id)
        .map_or_else(|| id.to_string(), |i| i.name.clone())
}

/// Numeric layer fields that are also timeline lanes.
pub const TRANSFORM_LANES: [&str; 8] = [
    "x", "y", "width", "height", "rotation", "scaleX", "scaleY", "opacity",
];

pub(crate) fn set_number(item: &mut Item, field: &str, v: f64) -> Result<(), CommandError> {
    match field {
        "x" => item.x = v,
        "y" => item.y = v,
        "width" => item.width = v.max(0.0),
        "height" => item.height = v.max(0.0),
        "rotation" => item.rotation = v,
        "scaleX" => item.scale_x = Some(v),
        "scaleY" => item.scale_y = Some(v),
        "opacity" => item.opacity = v.clamp(0.0, 1.0),
        "pivotX" => item.pivot_x = Some(v),
        "pivotY" => item.pivot_y = Some(v),
        "fontSize" => item.font_size = Some(v.max(1.0)),
        "chartReveal" => item.chart_reveal = Some(v.clamp(0.0, 1.0)),
        "patternPhase" => item.pattern_phase = Some(v),
        "particleSpawnRate" => item.particle_spawn_rate = Some(v.max(0.0)),
        "particleEmitterX" => item.particle_emitter_x = Some(v),
        "particleEmitterY" => item.particle_emitter_y = Some(v),
        "particleEmitting" => item.particle_emitting = Some(v >= 0.5),
        "lightningEmitting" => item.lightning_emitting = Some(v >= 0.5),
        "lightningAx" => item.lightning_ax = Some(v),
        "lightningAy" => item.lightning_ay = Some(v),
        "lightningBx" => item.lightning_bx = Some(v),
        "lightningBy" => item.lightning_by = Some(v),
        "borderWidth" => item.border_width = v.max(0.0),
        _ if crate::modifiers::set_lane(item, field, v)? => {}
        _ => return Err(CommandError::UnknownField(field.to_string())),
    }
    Ok(())
}

/// Sets a layer's own value of a numeric field (`x`, `opacity`, `pivotX`, …).
/// Keyframes, if any, still override it while animating.
pub struct SetNumber {
    pub item: String,
    pub field: String,
    pub value: f64,
}

impl Command for SetNumber {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        set_number(item_mut(doc, &self.item)?, &self.field, self.value)?;
        Ok(Change::Pose)
    }
    fn label(&self, doc: &Document) -> String {
        format!("Change {} of {}", self.field, item_name(doc, &self.item))
    }
}

/// Sets a value at the playhead:
///
/// - Auto-key on: writes a keyframe there (animating the lane if it was not).
/// - Auto-key off, lane animated: shifts every keyframe of the lane by the
///   same amount, so the animation keeps its shape and passes through the
///   new value at the playhead. No keyframes are added.
/// - Auto-key off, not animated: changes the layer's own value.
pub struct SetValueAt {
    pub item: String,
    pub lane: String,
    pub time_ms: f64,
    pub value: f64,
    /// The timeline's Auto-key switch.
    pub auto_key: bool,
}

impl Command for SetValueAt {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        if self.auto_key {
            return SetKeyframe {
                item: self.item.clone(),
                lane: self.lane.clone(),
                time_ms: self.time_ms,
                value: self.value,
                easing: None,
            }
            .apply(doc);
        }
        let Some(current) =
            lane_keys(doc, &self.item, &self.lane).and_then(|k| value_at(k, self.time_ms))
        else {
            return SetNumber {
                item: self.item.clone(),
                field: self.lane.clone(),
                value: self.value,
            }
            .apply(doc);
        };
        let delta = self.value - current;
        item_mut(doc, &self.item)?;
        let keys = track_mut(doc, &self.item)
            .properties
            .get_mut(&self.lane)
            .expect("animated lane");
        for k in keys.iter_mut() {
            k.value += delta;
        }
        // Keep the layer's own value with the animation, so removing the
        // keyframes later does not jump back.
        if let Some(base) = doc
            .items
            .iter()
            .find(|i| i.id == self.item)
            .and_then(|i| base_value(i, &self.lane))
        {
            set_number(item_mut(doc, &self.item)?, &self.lane, base + delta)?;
        }
        Ok(Change::Pose)
    }
    fn label(&self, doc: &Document) -> String {
        format!("Change {} of {}", self.lane, item_name(doc, &self.item))
    }
}

/// Text of a text layer.
pub struct SetText {
    pub item: String,
    pub text: String,
}

impl Command for SetText {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        item_mut(doc, &self.item)?.text_content = Some(self.text.clone());
        Ok(Change::Layer(vec![self.item.clone()]))
    }
    fn label(&self, doc: &Document) -> String {
        format!("Edit text of {}", item_name(doc, &self.item))
    }
}

pub struct Rename {
    pub item: String,
    pub name: String,
}

impl Command for Rename {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        item_mut(doc, &self.item)?.name = self.name.clone();
        Ok(Change::None)
    }
    fn label(&self, doc: &Document) -> String {
        format!("Rename {}", item_name(doc, &self.item))
    }
}

/// Shows or hides a layer (hidden layers are left out of exports).
pub struct SetVisible {
    pub item: String,
    pub visible: bool,
}

impl Command for SetVisible {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        item_mut(doc, &self.item)?.visible = Some(self.visible);
        Ok(Change::Rebuild)
    }
    fn label(&self, doc: &Document) -> String {
        format!(
            "{} {}",
            if self.visible { "Show" } else { "Hide" },
            item_name(doc, &self.item)
        )
    }
}

pub struct SetLocked {
    pub item: String,
    pub locked: bool,
}

impl Command for SetLocked {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        item_mut(doc, &self.item)?.locked = Some(self.locked);
        Ok(Change::None)
    }
    fn label(&self, doc: &Document) -> String {
        format!(
            "{} {}",
            if self.locked { "Lock" } else { "Unlock" },
            item_name(doc, &self.item)
        )
    }
}

/// Keys closer than this (ms) count as the same key.
const SAME_TIME_MS: f64 = 0.5;

pub(crate) fn track_mut<'a>(doc: &'a mut Document, item: &str) -> &'a mut ItemTrack {
    let tracks = &mut doc.timeline.item_tracks;
    let i = match tracks.iter().position(|t| t.item_id == item) {
        Some(i) => i,
        None => {
            tracks.push(ItemTrack {
                item_id: item.to_string(),
                properties: BTreeMap::new(),
                enabled_properties: Some(BTreeMap::new()),
                extra: Default::default(),
            });
            tracks.len() - 1
        }
    };
    &mut tracks[i]
}

/// Adds a keyframe, or replaces the value (and easing, if given) of the key
/// at that time. The lane is switched on.
pub struct SetKeyframe {
    pub item: String,
    pub lane: String,
    pub time_ms: f64,
    pub value: f64,
    /// Easing of the move arriving at this key; `None` keeps the existing
    /// key's easing, or `linear` for a new key.
    pub easing: Option<String>,
}

impl Command for SetKeyframe {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        item_mut(doc, &self.item)?;
        let track = track_mut(doc, &self.item);
        let keys = track.properties.entry(self.lane.clone()).or_default();
        match keys
            .iter_mut()
            .find(|k| (k.time - self.time_ms).abs() < SAME_TIME_MS)
        {
            Some(k) => {
                k.value = self.value;
                if let Some(e) = &self.easing {
                    k.easing = Some(e.clone());
                }
            }
            None => {
                keys.push(Keyframe {
                    time: self.time_ms.max(0.0),
                    value: self.value,
                    easing: Some(self.easing.clone().unwrap_or_else(|| "linear".into())),
                    extra: Default::default(),
                });
                keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            }
        }
        track
            .enabled_properties
            .get_or_insert_default()
            .insert(self.lane.clone(), true);
        Ok(Change::Pose)
    }
    fn label(&self, doc: &Document) -> String {
        format!("Keyframe {} of {}", self.lane, item_name(doc, &self.item))
    }
}

fn find_key(doc: &Document, item: &str, lane: &str, time_ms: f64) -> Result<usize, CommandError> {
    doc.timeline
        .item_tracks
        .iter()
        .find(|t| t.item_id == item)
        .and_then(|t| t.properties.get(lane))
        .and_then(|keys| {
            keys.iter()
                .position(|k| (k.time - time_ms).abs() < SAME_TIME_MS)
        })
        .ok_or_else(|| CommandError::NoSuchKeyframe {
            item: item.to_string(),
            lane: lane.to_string(),
            time_ms: crate::js::number_to_string(time_ms),
        })
}

/// Removes a keyframe. Removing the last key of a lane also sets the
/// layer's own value to the key's value, so nothing jumps.
pub struct RemoveKeyframe {
    pub item: String,
    pub lane: String,
    pub time_ms: f64,
}

impl Command for RemoveKeyframe {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        let i = find_key(doc, &self.item, &self.lane, self.time_ms)?;
        let track = track_mut(doc, &self.item);
        let keys = track.properties.get_mut(&self.lane).expect("found above");
        let removed = keys.remove(i);
        if keys.is_empty() {
            track.properties.remove(&self.lane);
            if let Some(e) = track.enabled_properties.as_mut() {
                e.remove(&self.lane);
            }
            if base_value(
                doc.items
                    .iter()
                    .find(|it| it.id == self.item)
                    .expect("checked"),
                &self.lane,
            )
            .is_some()
            {
                let _ = set_number(item_mut(doc, &self.item)?, &self.lane, removed.value);
            }
        }
        Ok(Change::Pose)
    }
    fn label(&self, doc: &Document) -> String {
        format!(
            "Delete {} keyframe of {}",
            self.lane,
            item_name(doc, &self.item)
        )
    }
}

/// Moves a keyframe in time (replacing a key already at the target time).
pub struct MoveKeyframe {
    pub item: String,
    pub lane: String,
    pub from_ms: f64,
    pub to_ms: f64,
}

impl Command for MoveKeyframe {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        let i = find_key(doc, &self.item, &self.lane, self.from_ms)?;
        let to = self.to_ms.max(0.0);
        let keys = track_mut(doc, &self.item)
            .properties
            .get_mut(&self.lane)
            .expect("found above");
        let mut key = keys.remove(i);
        key.time = to;
        keys.retain(|k| (k.time - to).abs() >= SAME_TIME_MS);
        keys.push(key);
        keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        Ok(Change::Pose)
    }
    fn label(&self, doc: &Document) -> String {
        format!(
            "Move {} keyframe of {}",
            self.lane,
            item_name(doc, &self.item)
        )
    }
}

/// Easing of the move arriving at a keyframe.
pub struct SetEasing {
    pub item: String,
    pub lane: String,
    pub time_ms: f64,
    pub easing: String,
}

impl Command for SetEasing {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        let i = find_key(doc, &self.item, &self.lane, self.time_ms)?;
        track_mut(doc, &self.item)
            .properties
            .get_mut(&self.lane)
            .expect("found above")[i]
            .easing = Some(crate::easing::normalize(Some(&self.easing)).to_string());
        Ok(Change::Pose)
    }
    fn label(&self, doc: &Document) -> String {
        format!("Easing of {} keyframe", item_name(doc, &self.item))
    }
}

/// Ids of an item and everything nested under it.
pub(crate) fn subtree(doc: &Document, id: &str) -> Vec<String> {
    let mut out = vec![id.to_string()];
    let mut i = 0;
    while i < out.len() {
        let parent = out[i].clone();
        for item in &doc.items {
            if item.parent_id.clone().flatten().as_deref() == Some(parent.as_str())
                && !out.contains(&item.id)
            {
                out.push(item.id.clone());
            }
        }
        i += 1;
    }
    out
}

/// Deletes layers with everything nested under them, their keyframes, and
/// modifier links pointing at them.
pub struct DeleteItems {
    pub items: Vec<String>,
}

impl Command for DeleteItems {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        for id in &self.items {
            if !doc.items.iter().any(|i| &i.id == id) {
                return Err(CommandError::NoSuchItem(id.clone()));
            }
        }
        let mut gone: Vec<String> = Vec::new();
        for id in &self.items {
            for sub in subtree(doc, id) {
                if !gone.contains(&sub) {
                    gone.push(sub);
                }
            }
        }
        doc.items.retain(|i| !gone.contains(&i.id));
        doc.timeline
            .item_tracks
            .retain(|t| !gone.contains(&t.item_id));
        // Masks and path follows that used a deleted layer are switched off.
        for item in &mut doc.items {
            for m in item.modifiers.iter_mut().flatten() {
                match m {
                    crate::model::Modifier::Mask(mask)
                        if mask
                            .source_element_id
                            .as_ref()
                            .is_some_and(|s| gone.contains(s)) =>
                    {
                        mask.source_element_id = None;
                    }
                    crate::model::Modifier::PathFollow(pf)
                        if gone.contains(&pf.source_element_id) =>
                    {
                        pf.enabled = false;
                    }
                    _ => {}
                }
            }
        }
        Ok(Change::Rebuild)
    }
    fn label(&self, doc: &Document) -> String {
        match self.items.as_slice() {
            [one] => format!("Delete {}", item_name(doc, one)),
            many => format!("Delete {} layers", many.len()),
        }
    }
}

/// Moves a layer under a new parent (or to the top level with `None`) and
/// to a position among its new siblings (`index`, 0 = drawn first).
///
/// With `keep_place` (a time), a layer that changes parent stays where it
/// is on screen at that time, as in FRAME: its position and rotation are
/// recomputed for the new parent, and their keyframes shift by the same
/// amount.
pub struct MoveItem {
    pub item: String,
    pub parent: Option<String>,
    pub index: usize,
    pub keep_place: Option<f64>,
}

/// Adds `delta` to a lane: every keyframe and the layer's own value.
fn shift_lane(doc: &mut Document, item: &str, lane: &str, delta: f64) -> Result<(), CommandError> {
    if delta == 0.0 {
        return Ok(());
    }
    if let Some(keys) = doc
        .timeline
        .item_tracks
        .iter_mut()
        .find(|t| t.item_id == item)
        .and_then(|t| t.properties.get_mut(lane))
    {
        for k in keys.iter_mut() {
            k.value += delta;
        }
    }
    let it = item_mut(doc, item)?;
    if let Some(base) = base_value(it, lane) {
        set_number(it, lane, base + delta)?;
    }
    Ok(())
}

impl Command for MoveItem {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        let from = doc
            .items
            .iter()
            .position(|i| i.id == self.item)
            .ok_or_else(|| CommandError::NoSuchItem(self.item.clone()))?;
        if let Some(p) = &self.parent {
            if !doc.items.iter().any(|i| &i.id == p) {
                return Err(CommandError::NoSuchItem(p.clone()));
            }
            if subtree(doc, &self.item).contains(p) {
                return Err(CommandError::WouldCreateCycle(self.item.clone()));
            }
        }
        // Where the layer's pivot is on screen, and its parent's rotation.
        let place = self.keep_place.map(|t| {
            use crate::manipulate::{box_to_parent, parent_frame};
            let item = &doc.items[from];
            let pose = crate::timeline::pose_at(doc, item, t);
            let frame = parent_frame(doc, &self.item, t);
            let c = (
                pose.width * pose.pivot_x / 100.0,
                pose.height * pose.pivot_y / 100.0,
            );
            (
                frame.to_scene(box_to_parent(&pose, c)),
                frame.ux.1.atan2(frame.ux.0).to_degrees(),
            )
        });
        let mut item = doc.items.remove(from);
        let old_parent = item.parent_id.clone().flatten();
        item.parent_id = Some(self.parent.clone());
        // Insert before the `index`-th sibling under the new parent.
        let siblings: Vec<usize> = doc
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.parent_id.clone().flatten() == self.parent)
            .map(|(pos, _)| pos)
            .collect();
        let at = match siblings.get(self.index) {
            Some(&pos) => pos,
            None => siblings.last().map_or(doc.items.len(), |&p| p + 1),
        };
        let reparented = item.parent_id.clone().flatten() != old_parent;
        doc.items.insert(at.min(doc.items.len()), item);
        if let (Some(t), true, Some((pivot, angle))) = (self.keep_place, reparented, place) {
            use crate::manipulate::{box_to_parent, parent_frame};
            let item = doc
                .items
                .iter()
                .find(|i| i.id == self.item)
                .expect("just inserted");
            let pose = crate::timeline::pose_at(doc, item, t);
            let frame = parent_frame(doc, &self.item, t);
            let local = frame.to_local(pivot);
            let c = (
                pose.width * pose.pivot_x / 100.0,
                pose.height * pose.pivot_y / 100.0,
            );
            let at_pivot = box_to_parent(&pose, c);
            let (dx, dy) = (local.0 - at_pivot.0, local.1 - at_pivot.1);
            let dr = angle - frame.ux.1.atan2(frame.ux.0).to_degrees();
            shift_lane(doc, &self.item, "x", dx)?;
            shift_lane(doc, &self.item, "y", dy)?;
            shift_lane(doc, &self.item, "rotation", dr)?;
        }
        Ok(Change::Rebuild)
    }
    fn label(&self, doc: &Document) -> String {
        format!("Move {}", item_name(doc, &self.item))
    }
}

/// Which part of the graphic a field belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldTarget {
    Item(String),
    Scene,
    Timeline,
    /// One modifier of a layer, by its id.
    Modifier {
        item: String,
        modifier: String,
    },
}

/// How much of the preview a field affects (see [`Change`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    None,
    Pose,
    Layer,
    Rebuild,
}

/// Sets one field by its JSON path (`fontSize`, `shadow.x`,
/// `borderRadius.0`). It goes through the model's own JSON form, so any
/// field of a layer, the scene or the timeline can be set; a value of the
/// wrong type is refused. `null` removes an optional field.
pub struct SetField {
    pub target: FieldTarget,
    pub path: String,
    pub value: serde_json::Value,
    pub update: Update,
}

fn set_path(
    root: &mut serde_json::Value,
    path: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    use serde_json::Value;
    let parts: Vec<&str> = path.split('.').collect();
    let mut node = root;
    for (i, part) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        node = match node {
            Value::Object(map) => {
                if last {
                    if value.is_null() {
                        map.remove(*part);
                    } else {
                        map.insert(part.to_string(), value);
                    }
                    return Ok(());
                }
                map.entry(part.to_string())
                    .or_insert_with(|| Value::Object(Default::default()))
            }
            Value::Array(list) => {
                let index: usize = part
                    .parse()
                    .map_err(|_| format!("{part} is not an index"))?;
                let slot = list
                    .get_mut(index)
                    .ok_or_else(|| format!("no element {index}"))?;
                if last {
                    *slot = value;
                    return Ok(());
                }
                slot
            }
            _ => return Err(format!("{path}: {part} is inside a plain value")),
        };
    }
    Ok(())
}

pub(crate) fn with_json<T>(
    target: &mut T,
    path: &str,
    value: serde_json::Value,
) -> Result<(), CommandError>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let mut json = serde_json::to_value(&*target).expect("model serializes");
    set_path(&mut json, path, value).map_err(CommandError::InvalidValue)?;
    *target = serde_json::from_value(json)
        .map_err(|e| CommandError::InvalidValue(format!("{path}: {e}")))?;
    Ok(())
}

impl Command for SetField {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        match &self.target {
            FieldTarget::Item(id) => with_json(item_mut(doc, id)?, &self.path, self.value.clone())?,
            FieldTarget::Scene => with_json(&mut doc.scene, &self.path, self.value.clone())?,
            FieldTarget::Timeline => with_json(&mut doc.timeline, &self.path, self.value.clone())?,
            FieldTarget::Modifier { item, modifier } => {
                let m = item_mut(doc, item)?
                    .modifiers
                    .iter_mut()
                    .flatten()
                    .find(|m| crate::modifiers::id(m) == modifier)
                    .ok_or_else(|| CommandError::InvalidValue(format!("no modifier {modifier}")))?;
                with_json(m, &self.path, self.value.clone())?;
            }
        }
        Ok(match (self.update, &self.target) {
            (Update::None, _) => Change::None,
            (Update::Pose, _) => Change::Pose,
            (Update::Layer, FieldTarget::Item(id) | FieldTarget::Modifier { item: id, .. }) => {
                Change::Layer(vec![id.clone()])
            }
            (Update::Layer | Update::Rebuild, _) => Change::Rebuild,
        })
    }
    fn label(&self, doc: &Document) -> String {
        let field = self.path.split('.').next().unwrap_or(&self.path);
        match &self.target {
            FieldTarget::Item(id) => format!("Change {field} of {}", item_name(doc, id)),
            FieldTarget::Scene => format!("Change scene {field}"),
            FieldTarget::Timeline => format!("Change timeline {field}"),
            FieldTarget::Modifier { item, .. } => {
                format!("Change modifier {field} of {}", item_name(doc, item))
            }
        }
    }
}

/// Several commands as one: all apply, or none (the document is put back
/// when one fails).
pub struct Batch {
    pub label: String,
    pub commands: Vec<Box<dyn Command>>,
}

impl Command for Batch {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        let before = doc.clone();
        let mut change = Change::None;
        for c in &self.commands {
            match c.apply(doc) {
                Ok(ch) => change = change.merge(ch),
                Err(e) => {
                    *doc = before;
                    return Err(e);
                }
            }
        }
        Ok(change)
    }
    fn label(&self, _doc: &Document) -> String {
        self.label.clone()
    }
}

/// Adds a layer on top (last in drawing order), with its parent set.
pub struct AddItem {
    pub item: Item,
}

impl Command for AddItem {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        if let Some(p) = self.item.parent_id.clone().flatten()
            && !doc.items.iter().any(|i| i.id == p)
        {
            return Err(CommandError::NoSuchItem(p));
        }
        doc.items.push(self.item.clone());
        // Lottie and video get their Start key.
        if crate::normalize::uses_clip_start(&self.item) {
            let start = match self.item.item_type {
                Some(crate::model::ItemType::Lottie) => self.item.lottie_start_ms,
                _ => self.item.video_start_ms,
            };
            crate::keys::set_clip_start(doc, &self.item.id, start.unwrap_or(0.0))?;
        }
        Ok(Change::Rebuild)
    }
    fn label(&self, _doc: &Document) -> String {
        format!("Add {}", self.item.name)
    }
}

/// Duplicates layers with everything under them and their keyframes. Copies
/// go on top, 20 px down and right, named "… Copy" (as in FRAME). Masks and
/// path follows between copied layers point at the copies.
pub struct DuplicateItems {
    pub items: Vec<String>,
    /// New ids, in the order the copies are made (filled by
    /// [`DuplicateItems::new`] so that redo makes the same ids).
    ids: Vec<String>,
}

impl DuplicateItems {
    pub fn new(doc: &Document, items: Vec<String>) -> Self {
        let roots = Self::roots(doc, &items);
        let count = roots.iter().map(|r| subtree(doc, r).len()).sum();
        Self {
            items,
            ids: (0..count).map(|_| crate::layers::new_id()).collect(),
        }
    }

    /// Selected layers that are not inside another selected layer.
    fn roots(doc: &Document, items: &[String]) -> Vec<String> {
        items
            .iter()
            .filter(|id| {
                !items
                    .iter()
                    .any(|other| other != *id && subtree(doc, other).contains(id))
            })
            .cloned()
            .collect()
    }

    /// Ids of the copies of the selected layers, after [`Command::apply`].
    pub fn copy_ids(&self, doc: &Document) -> Vec<String> {
        let roots = Self::roots(doc, &self.items);
        let mut out = Vec::new();
        let mut next = 0;
        for r in &roots {
            if let Some(id) = self.ids.get(next) {
                out.push(id.clone());
            }
            next += subtree(doc, r).len();
        }
        out
    }
}

impl Command for DuplicateItems {
    fn apply(&self, doc: &mut Document) -> Result<Change, CommandError> {
        for id in &self.items {
            if !doc.items.iter().any(|i| &i.id == id) {
                return Err(CommandError::NoSuchItem(id.clone()));
            }
        }
        let roots = Self::roots(doc, &self.items);
        let mut map: Vec<(String, String)> = Vec::new();
        let mut fresh = self.ids.iter();
        for r in &roots {
            for old in subtree(doc, r) {
                let new = fresh.next().cloned().unwrap_or_else(crate::layers::new_id);
                map.push((old, new));
            }
        }
        let lookup = |id: &str| map.iter().find(|(o, _)| o == id).map(|(_, n)| n.clone());
        let mut copies = Vec::new();
        // In drawing order, so the copies keep the originals' stacking.
        for item in doc.items.iter().filter(|i| lookup(&i.id).is_some()) {
            let mut c = item.clone();
            c.id = lookup(&item.id).expect("mapped");
            if let Some(p) = item.parent_id.clone().flatten().and_then(|p| lookup(&p)) {
                c.parent_id = Some(Some(p));
            }
            if roots.contains(&item.id) {
                c.x += 20.0;
                c.y += 20.0;
                let base = item.name.trim();
                let base = if base.is_empty() { "Item" } else { base };
                c.name = if base.to_lowercase().ends_with(" copy") {
                    format!("{base} 2")
                } else {
                    format!("{base} Copy")
                };
            }
            for m in c.modifiers.iter_mut().flatten() {
                match m {
                    crate::model::Modifier::Mask(mask) => {
                        mask.id = crate::layers::new_id();
                        if let Some(n) = mask.source_element_id.as_deref().and_then(lookup) {
                            mask.source_element_id = Some(n);
                        }
                    }
                    crate::model::Modifier::PathFollow(pf) => {
                        pf.id = crate::layers::new_id();
                        if let Some(n) = lookup(&pf.source_element_id) {
                            pf.source_element_id = n;
                        }
                    }
                    _ => {}
                }
            }
            copies.push(c);
        }
        let tracks: Vec<ItemTrack> = doc
            .timeline
            .item_tracks
            .iter()
            .filter_map(|t| {
                lookup(&t.item_id).map(|n| ItemTrack {
                    item_id: n,
                    ..t.clone()
                })
            })
            .collect();
        doc.items.extend(copies);
        doc.timeline.item_tracks.extend(tracks);
        Ok(Change::Rebuild)
    }
    fn label(&self, doc: &Document) -> String {
        match self.items.as_slice() {
            [one] => format!("Duplicate {}", item_name(doc, one)),
            many => format!("Duplicate {} layers", many.len()),
        }
    }
}

/// One undo step.
struct Step {
    label: String,
    before: Document,
    change: Change,
}

/// Undo and redo for one graphic.
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// Open gesture: commands join this step instead of making new ones.
    gesture: Option<Step>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            gesture: None,
            limit: 200,
        }
    }
}

impl History {
    /// Runs a command. Outside a gesture it becomes its own undo step.
    pub fn execute(
        &mut self,
        doc: &mut Document,
        cmd: &dyn Command,
    ) -> Result<Change, CommandError> {
        let label = cmd.label(doc);
        let before = (self.gesture.is_none()).then(|| doc.clone());
        let change = cmd.apply(doc)?;
        match (&mut self.gesture, before) {
            (Some(g), _) => {
                g.change = std::mem::replace(&mut g.change, Change::None).merge(change.clone())
            }
            (None, Some(before)) => self.push(Step {
                label,
                before,
                change: change.clone(),
            }),
            (None, None) => unreachable!("snapshot is taken outside gestures"),
        }
        Ok(change)
    }

    /// Starts a gesture (a drag, a scrubbed number): everything until
    /// [`History::end_gesture`] is one undo step.
    pub fn begin_gesture(&mut self, doc: &Document, label: impl Into<String>) {
        self.end_gesture(doc);
        self.gesture = Some(Step {
            label: label.into(),
            before: doc.clone(),
            change: Change::None,
        });
    }

    /// Closes the open gesture. A gesture that changed nothing leaves no step.
    pub fn end_gesture(&mut self, doc: &Document) {
        if let Some(step) = self.gesture.take()
            && &step.before != doc
        {
            self.push(step);
        }
    }

    fn push(&mut self, step: Step) {
        self.undo.push(step);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }

    /// Restores the document before the last step. Returns what to update.
    pub fn undo(&mut self, doc: &mut Document) -> Option<Change> {
        self.end_gesture(doc);
        let step = self.undo.pop()?;
        let after = std::mem::replace(doc, step.before);
        let change = step.change.clone();
        self.redo.push(Step {
            label: step.label,
            before: after,
            change: step.change,
        });
        Some(change)
    }

    pub fn redo(&mut self, doc: &mut Document) -> Option<Change> {
        self.end_gesture(doc);
        let step = self.redo.pop()?;
        let before = std::mem::replace(doc, step.before);
        let change = step.change.clone();
        self.undo.push(Step {
            label: step.label,
            before,
            change: step.change,
        });
        Some(change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Graphic;
    use crate::normalize::normalize;
    use crate::profile::Profile;
    use crate::timeline::lane_value;
    use serde_json::json;

    fn item(id: &str, parent: Option<&str>) -> serde_json::Value {
        json!({
            "id": id, "name": id.to_uppercase(), "parentId": parent, "itemType": "shape",
            "x": 0, "y": 0, "width": 100, "height": 50, "rotation": 0, "opacity": 1, "bg_color": "#fff",
            "borderWidth": 0, "borderColor": "#000", "borderRadius": [0, 0, 0, 0],
            "shadow": { "color": "#000", "x": 0, "y": 0, "blur": 0 }
        })
    }

    fn doc() -> Document {
        let g: Graphic = serde_json::from_value(json!({
            "scene": { "id": "scene", "name": "S", "backgroundColor": "#000", "width": 1920, "height": 1080 },
            "items": [item("a", None), item("b", Some("a")), item("c", None), item("d", Some("b"))],
            "timeline": { "duration": 2000, "fps": 25, "itemTracks": [
                { "itemId": "c", "properties": { "x": [ { "time": 0, "value": 0 }, { "time": 1000, "value": 100 } ] } }
            ] }
        }))
        .unwrap();
        normalize(&g, &Profile::framecraft())
    }

    fn ids(d: &Document) -> Vec<&str> {
        d.items.iter().map(|i| i.id.as_str()).collect()
    }

    #[test]
    fn undo_and_redo_restore_exactly() {
        let mut d = doc();
        let original = d.clone();
        let mut h = History::default();
        h.execute(
            &mut d,
            &SetNumber {
                item: "a".into(),
                field: "x".into(),
                value: 42.0,
            },
        )
        .unwrap();
        h.execute(
            &mut d,
            &Rename {
                item: "a".into(),
                name: "Logo".into(),
            },
        )
        .unwrap();
        let edited = d.clone();
        assert_eq!(h.undo_label(), Some("Rename A"));
        assert_eq!(h.undo(&mut d), Some(Change::None));
        assert_eq!(h.undo(&mut d), Some(Change::Pose));
        assert_eq!(d, original);
        assert_eq!(h.undo(&mut d), None);
        h.redo(&mut d);
        h.redo(&mut d);
        assert_eq!(d, edited);
    }

    #[test]
    fn a_gesture_is_one_step() {
        let mut d = doc();
        let original = d.clone();
        let mut h = History::default();
        h.begin_gesture(&d, "Move A");
        for x in 1..=30 {
            h.execute(
                &mut d,
                &SetNumber {
                    item: "a".into(),
                    field: "x".into(),
                    value: x as f64,
                },
            )
            .unwrap();
        }
        h.end_gesture(&d);
        assert_eq!(h.undo_label(), Some("Move A"));
        h.undo(&mut d);
        assert_eq!(d, original);
        assert_eq!(h.undo_label(), None);
        // An empty gesture leaves nothing to undo.
        h.begin_gesture(&d, "Nothing");
        h.end_gesture(&d);
        assert_eq!(h.undo_label(), None);
    }

    #[test]
    fn failed_commands_change_nothing() {
        let mut d = doc();
        let original = d.clone();
        let mut h = History::default();
        assert!(
            h.execute(
                &mut d,
                &SetNumber {
                    item: "zzz".into(),
                    field: "x".into(),
                    value: 1.0
                }
            )
            .is_err()
        );
        assert!(
            h.execute(
                &mut d,
                &SetNumber {
                    item: "a".into(),
                    field: "bogus".into(),
                    value: 1.0
                }
            )
            .is_err()
        );
        assert!(
            h.execute(
                &mut d,
                &MoveItem {
                    keep_place: None,
                    item: "a".into(),
                    parent: Some("d".into()),
                    index: 0
                }
            )
            .is_err()
        );
        assert_eq!(d, original);
        assert_eq!(h.undo_label(), None);
    }

    fn set_at(lane: &str, time_ms: f64, value: f64, auto_key: bool) -> SetValueAt {
        SetValueAt {
            item: "c".into(),
            lane: lane.into(),
            time_ms,
            value,
            auto_key,
        }
    }

    #[test]
    fn auto_key_on_writes_keyframes() {
        let mut d = doc();
        let mut h = History::default();
        h.execute(&mut d, &set_at("x", 500.0, 7.0, true)).unwrap();
        assert_eq!(lane_value(&d, "c", "x", 500.0), Some(7.0));
        assert_eq!(lane_keys(&d, "c", "x").unwrap().len(), 3);
        // The first edit of a lane starts animating it.
        h.execute(&mut d, &set_at("y", 800.0, 12.0, true)).unwrap();
        assert_eq!(lane_keys(&d, "c", "y").unwrap().len(), 1);
        assert_eq!(lane_value(&d, "c", "y", 0.0), Some(12.0));
    }

    #[test]
    fn auto_key_off_shifts_animations_and_sets_plain_values() {
        let mut d = doc();
        let mut h = History::default();
        // c.x goes 0 -> 100 over 1 s; at 500 ms it is 50. Ask for 80 there.
        h.execute(&mut d, &set_at("x", 500.0, 80.0, false)).unwrap();
        let keys: Vec<(f64, f64)> = lane_keys(&d, "c", "x")
            .unwrap()
            .iter()
            .map(|k| (k.time, k.value))
            .collect();
        assert_eq!(keys, vec![(0.0, 30.0), (1000.0, 130.0)]);
        assert_eq!(lane_value(&d, "c", "x", 500.0), Some(80.0));
        // Repeated absolute requests (a drag) land exactly, without drift.
        h.execute(&mut d, &set_at("x", 500.0, 90.0, false)).unwrap();
        h.execute(&mut d, &set_at("x", 500.0, 90.0, false)).unwrap();
        assert_eq!(lane_value(&d, "c", "x", 500.0), Some(90.0));
        assert_eq!(lane_keys(&d, "c", "x").unwrap().len(), 2);
        // Not animated: the layer's own value.
        h.execute(&mut d, &set_at("y", 500.0, 9.0, false)).unwrap();
        assert_eq!(d.items.iter().find(|i| i.id == "c").unwrap().y, 9.0);
        assert!(lane_keys(&d, "c", "y").is_none());
    }

    #[test]
    fn keyframes_set_move_ease_and_remove() {
        let mut d = doc();
        let mut h = History::default();
        h.execute(
            &mut d,
            &SetKeyframe {
                item: "a".into(),
                lane: "opacity".into(),
                time_ms: 0.0,
                value: 0.0,
                easing: None,
            },
        )
        .unwrap();
        h.execute(
            &mut d,
            &SetKeyframe {
                item: "a".into(),
                lane: "opacity".into(),
                time_ms: 400.0,
                value: 1.0,
                easing: None,
            },
        )
        .unwrap();
        h.execute(
            &mut d,
            &SetEasing {
                item: "a".into(),
                lane: "opacity".into(),
                time_ms: 400.0,
                easing: "ease-in-quad".into(),
            },
        )
        .unwrap();
        assert!((lane_value(&d, "a", "opacity", 200.0).unwrap() - 0.25).abs() < 1e-9);
        h.execute(
            &mut d,
            &MoveKeyframe {
                item: "a".into(),
                lane: "opacity".into(),
                from_ms: 400.0,
                to_ms: 800.0,
            },
        )
        .unwrap();
        assert!((lane_value(&d, "a", "opacity", 400.0).unwrap() - 0.25).abs() < 1e-9);
        h.execute(
            &mut d,
            &RemoveKeyframe {
                item: "a".into(),
                lane: "opacity".into(),
                time_ms: 0.0,
            },
        )
        .unwrap();
        h.execute(
            &mut d,
            &RemoveKeyframe {
                item: "a".into(),
                lane: "opacity".into(),
                time_ms: 800.0,
            },
        )
        .unwrap();
        // The last key's value stays as the layer's own value.
        assert!(lane_keys(&d, "a", "opacity").is_none());
        assert_eq!(d.items[0].opacity, 1.0);
        assert!(
            h.execute(
                &mut d,
                &RemoveKeyframe {
                    item: "a".into(),
                    lane: "opacity".into(),
                    time_ms: 5.0
                }
            )
            .is_err()
        );
    }

    #[test]
    fn delete_takes_children_and_tracks() {
        let mut d = doc();
        let mut h = History::default();
        let change = h
            .execute(
                &mut d,
                &DeleteItems {
                    items: vec!["a".into()],
                },
            )
            .unwrap();
        assert_eq!(change, Change::Rebuild);
        assert_eq!(ids(&d), vec!["c"]);
        assert_eq!(h.undo_label(), Some("Delete A"));
        h.undo(&mut d);
        assert_eq!(ids(&d), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn move_reorders_and_reparents() {
        let mut d = doc();
        let mut h = History::default();
        h.execute(
            &mut d,
            &MoveItem {
                keep_place: None,
                item: "c".into(),
                parent: None,
                index: 0,
            },
        )
        .unwrap();
        assert_eq!(ids(&d), vec!["c", "a", "b", "d"]);
        h.execute(
            &mut d,
            &MoveItem {
                keep_place: None,
                item: "c".into(),
                parent: Some("a".into()),
                index: 5,
            },
        )
        .unwrap();
        assert_eq!(
            d.items.iter().find(|i| i.id == "c").unwrap().parent_id,
            Some(Some("a".into()))
        );
        assert_eq!(ids(&d), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn reparent_keeps_place_on_screen() {
        use crate::manipulate::{corners, parent_frame};
        use crate::timeline::pose_at;
        let mut d = doc();
        let mut h = History::default();
        {
            let a = d.items.iter_mut().find(|i| i.id == "a").unwrap();
            (a.x, a.y, a.rotation) = (300.0, 200.0, 90.0);
        }
        let seen = |d: &Document, t: f64| {
            let c = d.items.iter().find(|i| i.id == "c").unwrap();
            corners(&pose_at(d, c, t), &parent_frame(d, "c", t))
        };
        // c is animated; the place is kept at 500 ms.
        let before = seen(&d, 500.0);
        for parent in [Some("a"), None] {
            h.execute(
                &mut d,
                &MoveItem {
                    item: "c".into(),
                    parent: parent.map(Into::into),
                    index: 0,
                    keep_place: Some(500.0),
                },
            )
            .unwrap();
            for (b, a) in before.iter().zip(seen(&d, 500.0).iter()) {
                assert!(
                    (b.0 - a.0).abs() < 1e-6 && (b.1 - a.1).abs() < 1e-6,
                    "{parent:?}: {b:?} vs {a:?}"
                );
            }
        }
    }

    #[test]
    fn set_field_by_path() {
        use serde_json::json;
        let mut d = doc();
        let mut h = History::default();
        let set = |path: &str, value: serde_json::Value| SetField {
            target: FieldTarget::Item("a".into()),
            path: path.into(),
            value,
            update: Update::Layer,
        };
        assert_eq!(
            h.execute(&mut d, &set("shadow.x", json!(4))).unwrap(),
            Change::Layer(vec!["a".into()])
        );
        h.execute(&mut d, &set("borderRadius.2", json!(9))).unwrap();
        h.execute(&mut d, &set("textColor", json!("#ff0000")))
            .unwrap();
        let a = d.items.iter().find(|i| i.id == "a").unwrap();
        assert_eq!(
            (a.shadow.x, a.border_radius[2], a.text_color.as_deref()),
            (4.0, 9.0, Some("#ff0000"))
        );
        // Wrong types are refused and change nothing.
        let before = d.clone();
        assert!(h.execute(&mut d, &set("opacity", json!("high"))).is_err());
        assert!(h.execute(&mut d, &set("borderRadius.9", json!(1))).is_err());
        assert_eq!(d, before);
        // null removes an optional field.
        h.execute(&mut d, &set("textColor", serde_json::Value::Null))
            .unwrap();
        assert_eq!(
            d.items.iter().find(|i| i.id == "a").unwrap().text_color,
            None
        );
        // Scene and timeline fields.
        let scene = SetField {
            target: FieldTarget::Scene,
            path: "width".into(),
            value: json!(1280),
            update: Update::Rebuild,
        };
        assert_eq!(h.execute(&mut d, &scene).unwrap(), Change::Rebuild);
        assert_eq!(d.scene.width, 1280.0);
        let tl = SetField {
            target: FieldTarget::Timeline,
            path: "duration".into(),
            value: json!(5000),
            update: Update::Rebuild,
        };
        h.execute(&mut d, &tl).unwrap();
        assert_eq!(d.timeline.duration, 5000.0);
    }

    #[test]
    fn add_and_duplicate() {
        let mut d = doc();
        let mut h = History::default();
        let item = crate::layers::new_layer(crate::layers::NewLayer::Text, Some("a"));
        let new_id = item.id.clone();
        h.execute(&mut d, &AddItem { item }).unwrap();
        assert_eq!(d.items.last().unwrap().id, new_id);
        // Duplicate "a" with its children b, d and the new text; c is not copied.
        let dup = DuplicateItems::new(&d, vec!["a".into(), "b".into()]);
        h.execute(&mut d, &dup).unwrap();
        assert_eq!(d.items.len(), 5 + 4);
        let copies = dup.copy_ids(&d);
        assert_eq!(copies.len(), 1);
        let copy = d.items.iter().find(|i| i.id == copies[0]).unwrap();
        assert_eq!((copy.name.as_str(), copy.x), ("A Copy", 20.0));
        // The copy of b sits under the copy of a.
        let b_copy = d
            .items
            .iter()
            .find(|i| i.name == "B" && i.id != "b")
            .unwrap();
        assert_eq!(b_copy.parent_id, Some(Some(copies[0].clone())));
        // Undo and redo make the same ids.
        let after = d.clone();
        h.undo(&mut d);
        h.redo(&mut d);
        assert_eq!(d, after);
        // c's keyframes were copied for... nothing: c was not duplicated.
        assert_eq!(d.timeline.item_tracks.len(), 1);
    }

    #[test]
    fn changes_merge_to_the_bigger_update() {
        assert_eq!(Change::Pose.merge(Change::None), Change::Pose);
        assert_eq!(
            Change::Layer(vec!["a".into()]).merge(Change::Layer(vec!["b".into(), "a".into()])),
            Change::Layer(vec!["a".into(), "b".into()])
        );
        assert_eq!(
            Change::Layer(vec!["a".into()]).merge(Change::Rebuild),
            Change::Rebuild
        );
    }
}
