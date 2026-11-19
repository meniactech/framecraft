// SPDX-License-Identifier: AGPL-3.0-or-later
//! The property schema: every editable property described once. The
//! properties panel draws its fields from it, edits go through the matching
//! command, and the preview update follows from it.

use crate::commands::{Command, FieldTarget, SetField, SetValueAt, Update};
use crate::model::{Item, ItemType, ShapeType};
use crate::normalize::Document;
use serde_json::Value;

/// Properties panel tabs (FRAME's names).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Scene,
    Transform,
    Look,
    Text,
}

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Tab::Scene => "Scene",
            Tab::Transform => "Transform",
            Tab::Look => "Look",
            Tab::Text => "Text",
        }
    }

    /// The icon file in FRAME's `static/icons/properties/`.
    pub fn icon(self) -> &'static str {
        match self {
            Tab::Scene => "scene",
            Tab::Transform => "transform",
            Tab::Look => "look",
            Tab::Text => "text",
        }
    }

    /// Tabs shown for a selected layer (Scene is always available).
    pub fn for_item(item: &Item) -> Vec<Tab> {
        let mut tabs = vec![Tab::Transform];
        if is_filled(item) {
            tabs.push(Tab::Look);
        }
        if is_text(item) {
            tabs.push(Tab::Text);
        }
        tabs
    }
}

/// What kind of field a property is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Number {
        min: f64,
        max: f64,
        step: f64,
    },
    Color,
    /// `(stored value, label)` pairs.
    Choice(&'static [(&'static str, &'static str)]),
    Toggle,
    Text,
    TextArea,
    /// Text stored as a number when it is one (`spxOut`: `manual` or ms).
    TextOrNumber,
    /// A font family: built-in fonts, or any family name.
    Font,
    /// Gradient stops: `[{offset, color}, …]`.
    Ramp,
}

/// Where a property's value lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Item,
    Scene,
    Timeline,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Property {
    pub tab: Tab,
    /// Section heading in the tab.
    pub group: &'static str,
    /// JSON path of the value (`fontSize`, `shadow.x`, `borderRadius.0`).
    pub path: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub scope: Scope,
    /// The timeline lane that animates it, if any.
    pub lane: Option<&'static str>,
    pub update: Update,
}

const fn num(min: f64, max: f64, step: f64) -> Kind {
    Kind::Number { min, max, step }
}

const ANY: f64 = 1e9;

pub const BLEND_MODES: &[(&str, &str)] = &[
    ("normal", "Normal"),
    ("multiply", "Multiply"),
    ("screen", "Screen"),
    ("overlay", "Overlay"),
    ("darken", "Darken"),
    ("lighten", "Lighten"),
    ("color-dodge", "Color Dodge"),
    ("color-burn", "Color Burn"),
    ("hard-light", "Hard Light"),
    ("soft-light", "Soft Light"),
    ("difference", "Difference"),
    ("exclusion", "Exclusion"),
    ("hue", "Hue"),
    ("saturation", "Saturation"),
    ("color", "Color"),
    ("luminosity", "Luminosity"),
];

/// Fonts that ship with FrameCraft.
pub fn builtin_font_families() -> impl Iterator<Item = &'static str> {
    crate::export::assets::BUILTIN_FONTS
        .iter()
        .map(|(family, _)| *family)
}

macro_rules! p {
    ($tab:ident, $group:literal, $path:literal, $label:literal, $kind:expr, $scope:ident, $lane:expr, $update:ident) => {
        Property {
            tab: Tab::$tab,
            group: $group,
            path: $path,
            label: $label,
            kind: $kind,
            scope: Scope::$scope,
            lane: $lane,
            update: Update::$update,
        }
    };
}

/// Every property, in panel order.
pub const PROPERTIES: &[Property] = &[
    // Scene
    p!(
        Scene,
        "Canvas",
        "width",
        "Width",
        num(1.0, 16384.0, 1.0),
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "Canvas",
        "height",
        "Height",
        num(1.0, 16384.0, 1.0),
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "Canvas",
        "backgroundColor",
        "Background",
        Kind::Color,
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "Timing",
        "duration",
        "Duration (ms)",
        num(100.0, 3_600_000.0, 10.0),
        Timeline,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "Timing",
        "fps",
        "FPS",
        num(1.0, 120.0, 1.0),
        Timeline,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "SPX template",
        "spxDescription",
        "Description",
        Kind::Text,
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "SPX template",
        "spxPlayserver",
        "Play server",
        Kind::Text,
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "SPX template",
        "spxPlaychannel",
        "Channel",
        num(1.0, 99.0, 1.0),
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "SPX template",
        "spxPlaylayer",
        "Layer",
        num(1.0, 99.0, 1.0),
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "SPX template",
        "spxWebplayout",
        "Web playout",
        num(1.0, 99.0, 1.0),
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "SPX template",
        "spxOut",
        "Out (manual or ms)",
        Kind::TextOrNumber,
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "SPX template",
        "spxUicolor",
        "UI color",
        num(0.0, 7.0, 1.0),
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "SPX template",
        "spxPreventToggle",
        "Prevent toggle",
        Kind::Toggle,
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "BOARD",
        "ionboardHookEnabled",
        "Live data",
        Kind::Toggle,
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "BOARD",
        "ionboardPanelEnabled",
        "SPX panel",
        Kind::Toggle,
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "BOARD",
        "ionboardMagicId",
        "Magic ID",
        Kind::Text,
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "BOARD",
        "ionboardProjectId",
        "Project ID",
        Kind::Text,
        Scene,
        None,
        Rebuild
    ),
    // Transform
    p!(
        Transform,
        "Layer",
        "name",
        "Name",
        Kind::Text,
        Item,
        None,
        None
    ),
    p!(
        Transform,
        "Position",
        "x",
        "X",
        num(-ANY, ANY, 1.0),
        Item,
        Some("x"),
        Pose
    ),
    p!(
        Transform,
        "Position",
        "y",
        "Y",
        num(-ANY, ANY, 1.0),
        Item,
        Some("y"),
        Pose
    ),
    p!(
        Transform,
        "Position",
        "width",
        "Width",
        num(0.0, ANY, 1.0),
        Item,
        Some("width"),
        Pose
    ),
    p!(
        Transform,
        "Position",
        "height",
        "Height",
        num(0.0, ANY, 1.0),
        Item,
        Some("height"),
        Pose
    ),
    p!(
        Transform,
        "Position",
        "rotation",
        "Rotation",
        num(-ANY, ANY, 1.0),
        Item,
        Some("rotation"),
        Pose
    ),
    p!(
        Transform,
        "Position",
        "scaleX",
        "Scale X",
        num(-ANY, ANY, 0.01),
        Item,
        Some("scaleX"),
        Pose
    ),
    p!(
        Transform,
        "Position",
        "scaleY",
        "Scale Y",
        num(-ANY, ANY, 0.01),
        Item,
        Some("scaleY"),
        Pose
    ),
    p!(
        Transform,
        "Position",
        "opacity",
        "Opacity",
        num(0.0, 1.0, 0.01),
        Item,
        Some("opacity"),
        Pose
    ),
    p!(
        Transform,
        "Pivot",
        "pivotX",
        "Pivot X %",
        num(-ANY, ANY, 1.0),
        Item,
        None,
        Pose
    ),
    p!(
        Transform,
        "Pivot",
        "pivotY",
        "Pivot Y %",
        num(-ANY, ANY, 1.0),
        Item,
        None,
        Pose
    ),
    p!(
        Transform,
        "Blending",
        "blendMode",
        "Blend mode",
        Kind::Choice(BLEND_MODES),
        Item,
        None,
        Layer
    ),
    p!(
        Transform,
        "Live visibility",
        "visibilityVariableId",
        "BOARD variable",
        Kind::Text,
        Item,
        None,
        Layer
    ),
    p!(
        Transform,
        "Live visibility",
        "visibilityInvert",
        "Invert",
        Kind::Toggle,
        Item,
        None,
        Layer
    ),
    // Look
    p!(
        Look,
        "Fill",
        "backgroundType",
        "Fill",
        Kind::Choice(&[
            ("solid", "Solid"),
            ("gradient", "Gradient"),
            ("image", "Image")
        ]),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Fill",
        "bg_color",
        "Color",
        Kind::Color,
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Fill",
        "gradientType",
        "Gradient",
        Kind::Choice(&[
            ("linear", "Linear"),
            ("circle", "Circle"),
            ("ellipse", "Ellipse"),
            ("conic", "Conic")
        ]),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Fill",
        "gradientAngle",
        "Angle",
        num(-360.0, 360.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Fill",
        "gradientRamp",
        "Stops",
        Kind::Ramp,
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Fill",
        "bgImageUrl",
        "Image URL",
        Kind::Text,
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Fill",
        "bgImageFit",
        "Image fit",
        Kind::Choice(&[
            ("cover", "Cover"),
            ("contain", "Contain"),
            ("stretch", "Stretch")
        ]),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Fill",
        "bgImagePosition",
        "Image position",
        Kind::Choice(&[
            ("center", "Center"),
            ("top", "Top"),
            ("bottom", "Bottom"),
            ("left", "Left"),
            ("right", "Right"),
            ("top-left", "Top left"),
            ("top-right", "Top right"),
            ("bottom-left", "Bottom left"),
            ("bottom-right", "Bottom right")
        ]),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Fill",
        "bgImageRepeat",
        "Image repeat",
        Kind::Choice(&[
            ("no-repeat", "No repeat"),
            ("repeat", "Tile"),
            ("repeat-x", "Tile X"),
            ("repeat-y", "Tile Y")
        ]),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Border",
        "borderWidth",
        "Width",
        num(0.0, 1000.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Border",
        "borderColor",
        "Color",
        Kind::Color,
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Corners",
        "borderRadius.0",
        "Top left",
        num(0.0, 9999.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Corners",
        "borderRadius.1",
        "Top right",
        num(0.0, 9999.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Corners",
        "borderRadius.2",
        "Bottom right",
        num(0.0, 9999.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Corners",
        "borderRadius.3",
        "Bottom left",
        num(0.0, 9999.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Shadow",
        "shadow.x",
        "X",
        num(-ANY, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Shadow",
        "shadow.y",
        "Y",
        num(-ANY, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Shadow",
        "shadow.blur",
        "Blur",
        num(0.0, 1000.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Shadow",
        "shadow.color",
        "Color",
        Kind::Color,
        Item,
        None,
        Layer
    ),
    // Text
    p!(
        Text,
        "Text",
        "textContent",
        "Text",
        Kind::TextArea,
        Item,
        None,
        Layer
    ),
    p!(
        Text,
        "Text",
        "textVariableId",
        "Variable ID",
        Kind::Text,
        Item,
        None,
        Rebuild
    ),
    p!(
        Text,
        "Font",
        "fontFamily",
        "Family",
        Kind::Font,
        Item,
        None,
        Layer
    ),
    p!(
        Text,
        "Font",
        "fontSize",
        "Size",
        num(8.0, 2000.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Text,
        "Font",
        "fontWeight",
        "Weight",
        Kind::Choice(&[
            ("100", "100 Thin"),
            ("200", "200 Extra light"),
            ("300", "300 Light"),
            ("400", "400 Regular"),
            ("500", "500 Medium"),
            ("600", "600 Semibold"),
            ("700", "700 Bold"),
            ("800", "800 Extra bold"),
            ("900", "900 Black")
        ]),
        Item,
        None,
        Layer
    ),
    p!(
        Text,
        "Font",
        "textAlign",
        "Align",
        Kind::Choice(&[("left", "Left"), ("center", "Center"), ("right", "Right")]),
        Item,
        None,
        Layer
    ),
    p!(
        Text,
        "Font",
        "textColor",
        "Color",
        Kind::Color,
        Item,
        None,
        Layer
    ),
];

fn is_text(item: &Item) -> bool {
    item.item_type == Some(ItemType::Text)
}

/// Layers with a fill and border: shapes, image slots and charts.
fn is_filled(item: &Item) -> bool {
    match item.item_type {
        Some(ItemType::Shape) | None => !is_text(item),
        Some(ItemType::ImageSlot | ItemType::Chart) => true,
        _ => false,
    }
}

impl Property {
    /// Properties of a tab, in order.
    pub fn of(tab: Tab) -> impl Iterator<Item = &'static Property> {
        PROPERTIES.iter().filter(move |p| p.tab == tab)
    }

    /// Whether this property is shown for `item`. Scene properties always
    /// are; some Look properties only make sense for some shapes.
    pub fn applies(&self, item: Option<&Item>) -> bool {
        let Some(item) = item else {
            return self.scope != Scope::Item;
        };
        if self.scope != Scope::Item {
            return false;
        }
        let shape = item.shape_type;
        match self.path {
            p if p.starts_with("borderRadius") => {
                is_filled(item)
                    && !matches!(
                        shape,
                        Some(
                            ShapeType::Ellipse
                                | ShapeType::Circle
                                | ShapeType::Polygon
                                | ShapeType::Path
                        )
                    )
            }
            p if p.starts_with("shadow") => {
                is_filled(item) && !matches!(shape, Some(ShapeType::Polygon | ShapeType::Path))
            }
            _ => match self.tab {
                Tab::Look => is_filled(item),
                Tab::Text => is_text(item),
                _ => true,
            },
        }
    }

    /// The stored value (`null` when not set).
    pub fn value(&self, doc: &Document, item: Option<&Item>) -> Value {
        let root = match (self.scope, item) {
            (Scope::Item, Some(i)) => serde_json::to_value(i),
            (Scope::Scene, _) => serde_json::to_value(&doc.scene),
            (Scope::Timeline, _) => serde_json::to_value(&doc.timeline),
            (Scope::Item, None) => return Value::Null,
        }
        .unwrap_or(Value::Null);
        self.path
            .split('.')
            .try_fold(&root, |v, part| match v {
                Value::Object(m) => m.get(part),
                Value::Array(a) => part.parse::<usize>().ok().and_then(|i| a.get(i)),
                _ => None,
            })
            .cloned()
            .unwrap_or(Value::Null)
    }

    /// The command that sets this property. Animatable numbers go through
    /// [`SetValueAt`] (keyframes, Auto-key); everything else through
    /// [`SetField`]. Text typed into a number field is parsed here.
    pub fn command(
        &self,
        item: Option<&str>,
        value: Value,
        time_ms: f64,
        auto_key: bool,
    ) -> Option<Box<dyn Command>> {
        let value = match (self.kind, value) {
            (Kind::Number { min, max, .. }, Value::String(s)) => {
                Value::from(s.trim().parse::<f64>().ok()?.clamp(min, max))
            }
            (Kind::Number { min, max, .. }, Value::Number(n)) => {
                Value::from(n.as_f64()?.clamp(min, max))
            }
            (Kind::TextOrNumber, Value::String(s)) => match s.trim().parse::<f64>() {
                Ok(n) => Value::from(n),
                Err(_) => Value::String(s.trim().to_string()),
            },
            (Kind::Choice(_), Value::String(s)) if self.path == "fontWeight" => {
                Value::from(s.parse::<f64>().ok()?)
            }
            (_, v) => v,
        };
        if let (Some(lane), Some(id)) = (self.lane, item) {
            return Some(Box::new(SetValueAt {
                item: id.into(),
                lane: lane.into(),
                time_ms,
                value: value.as_f64()?,
                auto_key,
            }));
        }
        let target = match self.scope {
            Scope::Item => FieldTarget::Item(item?.to_string()),
            Scope::Scene => FieldTarget::Scene,
            Scope::Timeline => FieldTarget::Timeline,
        };
        Some(Box::new(SetField {
            target,
            path: self.path.into(),
            value,
            update: self.update,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::History;
    use crate::layers::{NewLayer, new_layer};
    use crate::model::Graphic;
    use crate::normalize::normalize;
    use crate::profile::Profile;

    fn doc_with(layers: &[NewLayer]) -> Document {
        let g: Graphic = serde_json::from_value(serde_json::json!({
            "scene": { "id": "scene", "name": "S", "backgroundColor": "#000", "width": 1920, "height": 1080 },
            "items": []
        }))
        .unwrap();
        let mut d = normalize(&g, &Profile::framecraft());
        for kind in layers {
            d.items.push(new_layer(*kind, None));
        }
        d
    }

    #[test]
    fn every_path_reads_and_writes_on_fitting_layers() {
        let mut d = doc_with(&NewLayer::ALL);
        let mut h = History::default();
        for p in PROPERTIES {
            let targets: Vec<Option<String>> = match p.scope {
                Scope::Item => d
                    .items
                    .iter()
                    .filter(|i| p.applies(Some(i)))
                    .map(|i| Some(i.id.clone()))
                    .collect(),
                _ => vec![None],
            };
            assert!(!targets.is_empty(), "{} applies to nothing", p.path);
            for target in targets {
                let item = target
                    .as_ref()
                    .and_then(|id| d.items.iter().find(|i| &i.id == id));
                let current = p.value(&d, item);
                // Writing back what is there (or a fitting sample) must work.
                let value = match (p.kind, current.clone()) {
                    (_, v) if !v.is_null() => v,
                    (Kind::Number { min, .. }, _) => Value::from(min.max(0.0)),
                    (Kind::Color, _) => Value::from("#123456"),
                    (Kind::Toggle, _) => Value::from(true),
                    (Kind::Ramp, _) => {
                        serde_json::json!([{ "offset": 0, "color": "#000" }, { "offset": 1, "color": "#fff" }])
                    }
                    (Kind::Choice(c), _) => Value::from(c[0].0),
                    _ => Value::from("x"),
                };
                let cmd = p
                    .command(target.as_deref(), value, 0.0, false)
                    .unwrap_or_else(|| panic!("{}", p.path));
                h.execute(&mut d, cmd.as_ref())
                    .unwrap_or_else(|e| panic!("{}: {e}", p.path));
            }
        }
    }

    #[test]
    fn tabs_per_layer_type() {
        let d = doc_with(&[NewLayer::Text, NewLayer::Box, NewLayer::Empty]);
        let tabs: Vec<Vec<Tab>> = d.items.iter().map(Tab::for_item).collect();
        assert_eq!(
            tabs,
            vec![
                vec![Tab::Transform, Tab::Text],
                vec![Tab::Transform, Tab::Look],
                vec![Tab::Transform]
            ]
        );
    }

    #[test]
    fn typed_values_are_parsed_and_clamped() {
        let mut d = doc_with(&[NewLayer::Box]);
        let id = d.items[0].id.clone();
        let mut h = History::default();
        let opacity = PROPERTIES.iter().find(|p| p.path == "opacity").unwrap();
        h.execute(
            &mut d,
            opacity
                .command(Some(&id), Value::from(" 7 "), 0.0, false)
                .unwrap()
                .as_ref(),
        )
        .unwrap();
        assert_eq!(d.items[0].opacity, 1.0);
        let out = PROPERTIES.iter().find(|p| p.path == "spxOut").unwrap();
        h.execute(
            &mut d,
            out.command(None, Value::from("5000"), 0.0, false)
                .unwrap()
                .as_ref(),
        )
        .unwrap();
        assert_eq!(out.value(&d, None), Value::from(5000.0));
        assert!(
            opacity
                .command(Some(&id), Value::from("lots"), 0.0, false)
                .is_none()
        );
    }
}
