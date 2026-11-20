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
    /// Script functions and globals (FRAME's Js tab).
    Script,
    /// Export (FRAME's Exp tab).
    Export,
    Transform,
    Look,
    Text,
    Chart,
    Qr,
    Particles,
    Lightning,
    Pattern,
    Lottie,
    Video,
    Modifiers,
}

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Tab::Scene => "Scene",
            Tab::Script => "Script",
            Tab::Export => "Export",
            Tab::Transform => "Transform",
            Tab::Look => "Look",
            Tab::Text => "Text",
            Tab::Chart => "Chart",
            Tab::Qr => "QR",
            Tab::Particles => "Particles",
            Tab::Lightning => "Lightning",
            Tab::Pattern => "Pattern",
            Tab::Lottie => "Lottie",
            Tab::Video => "Video",
            Tab::Modifiers => "Modifiers",
        }
    }

    /// The icon file in FRAME's `static/icons/properties/`.
    pub fn icon(self) -> &'static str {
        match self {
            Tab::Scene => "scene",
            Tab::Script => "script",
            Tab::Export => "export",
            Tab::Transform => "transform",
            Tab::Look => "look",
            Tab::Text => "text",
            Tab::Chart | Tab::Qr => "chart",
            Tab::Particles | Tab::Lightning | Tab::Pattern => "particles",
            Tab::Lottie | Tab::Video => "look",
            Tab::Modifiers => "modifiers",
        }
    }

    /// Tabs shown for a selected layer (Scene is always available).
    pub fn for_item(item: &Item) -> Vec<Tab> {
        let mut tabs = vec![Tab::Transform];
        if is_filled(item) {
            tabs.push(Tab::Look);
        }
        match item.item_type {
            Some(ItemType::Text) => tabs.push(Tab::Text),
            Some(ItemType::Chart) => tabs.push(Tab::Chart),
            Some(ItemType::Qr) => tabs.push(Tab::Qr),
            Some(ItemType::Particles) => tabs.push(Tab::Particles),
            Some(ItemType::Lightning) => tabs.push(Tab::Lightning),
            Some(ItemType::Pattern) => tabs.push(Tab::Pattern),
            Some(ItemType::Lottie) => tabs.push(Tab::Lottie),
            Some(ItemType::Video) => tabs.push(Tab::Video),
            _ => {}
        }
        tabs.push(Tab::Modifiers);
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
    /// Chart data: `[{id, label, value, color?, liveVariableId?}, …]`.
    ChartPoints,
    /// Numbers along a 0–1 ramp: `[{offset, value}, …]` (values 0–1).
    ValueRamp,
    /// A list of colors.
    ColorList,
    /// A random seed (whole number), with a Randomize button.
    Seed,
    /// When a Lottie / video clip starts (ms); moves its timeline Start key.
    ClipStart,
    /// Another layer to mask with, or none (a custom region).
    MaskSource,
    /// A path or polygon layer to follow.
    PathSource,
    /// A mask's polygon: `[{x, y}, …]` in the layer's own box.
    MaskPoints,
    /// A media URL, picked from the project's media of this category
    /// (`images`, `videos`, `lottie`) or typed.
    Media(&'static str),
    /// A polygon or path's points, with bezier handles.
    PolygonPoints,
    /// A button that turns a tinted SVG image into a polygon layer.
    SvgToPolygon,
    /// Custom ruler positions in percent (0–100) of the scene.
    Rulers,
    /// A radial / conic gradient's center (`{x, y}`, 0–1), FRAME's pad.
    GradientCenter,
    /// A time in ms, shown and typed in the chosen time format.
    Time {
        min: f64,
        max: f64,
    },
}

/// Where a property's value lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Item,
    Scene,
    Timeline,
    /// Inside one modifier of a layer (see [`crate::modifiers`]).
    Modifier,
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
        "Duration",
        Kind::Time {
            min: 100.0,
            max: 3_600_000.0
        },
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
        "Enable BOARD Hook",
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
        "BOARD ID",
        Kind::Text,
        Scene,
        None,
        Rebuild
    ),
    p!(
        Scene,
        "BOARD",
        "ionboardPanelEnabled",
        "Add SPX Panel",
        Kind::Toggle,
        Scene,
        None,
        Rebuild
    ),
    // Editor guides (Scene → Guides). Editor-only; never exported.
    p!(
        Scene,
        "Guides",
        "guideLineWidth",
        "Line width (px)",
        num(1.0, 20.0, 1.0),
        Scene,
        None,
        None
    ),
    p!(
        Scene,
        "Guides",
        "guideColorSafe10",
        "Safe 10%",
        Kind::Color,
        Scene,
        None,
        None
    ),
    p!(
        Scene,
        "Guides",
        "guideColorSafe20",
        "Safe 20%",
        Kind::Color,
        Scene,
        None,
        None
    ),
    p!(
        Scene,
        "Guides",
        "guideColorThirds",
        "Thirds",
        Kind::Color,
        Scene,
        None,
        None
    ),
    p!(
        Scene,
        "Guides",
        "guideColorCenter",
        "Center +",
        Kind::Color,
        Scene,
        None,
        None
    ),
    p!(
        Scene,
        "Guides",
        "customRulerColor",
        "Rulers",
        Kind::Color,
        Scene,
        None,
        None
    ),
    p!(
        Scene,
        "Custom rulers",
        "customRulersH",
        "Horizontal (% from top)",
        Kind::Rulers,
        Scene,
        None,
        None
    ),
    p!(
        Scene,
        "Custom rulers",
        "customRulersV",
        "Vertical (% from left)",
        Kind::Rulers,
        Scene,
        None,
        None
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
        Layer
    ),
    p!(
        Transform,
        "Pivot",
        "pivotY",
        "Pivot Y %",
        num(-ANY, ANY, 1.0),
        Item,
        None,
        Layer
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
        "gradientCenter",
        "Center",
        Kind::GradientCenter,
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
        "Image",
        Kind::Media("images"),
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
        "borderWidthMode",
        "Mode",
        Kind::Choice(&[("single", "Single"), ("sides", "4-sided")]),
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
        Some("borderWidth"),
        Layer
    ),
    p!(
        Look,
        "Border",
        "borderWidths.0",
        "Top",
        num(0.0, 1000.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Border",
        "borderWidths.1",
        "Right",
        num(0.0, 1000.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Border",
        "borderWidths.2",
        "Bottom",
        num(0.0, 1000.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Border",
        "borderWidths.3",
        "Left",
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
    // Polygon and path shapes
    p!(
        Look,
        "Points",
        "polygonCurve",
        "Curves",
        Kind::Choice(&[("linear", "Linear"), ("bezier", "Bezier")]),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Points",
        "polygonPoints",
        "Points",
        Kind::PolygonPoints,
        Item,
        None,
        Layer
    ),
    // Image layers
    p!(
        Look,
        "Image",
        "mediaUrl",
        "Image",
        Kind::Media("images"),
        Item,
        None,
        Rebuild
    ),
    p!(
        Look,
        "Image",
        "svgTint",
        "Tint with fill color",
        Kind::Toggle,
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Image",
        "svgToPolygon",
        "Convert to polygon",
        Kind::SvgToPolygon,
        Item,
        None,
        Rebuild
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
    // Look: image slot (SPX picks the file by Variable ID).
    p!(
        Look,
        "Image slot",
        "imageVariableId",
        "Variable ID",
        Kind::Text,
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Image slot",
        "imageAssetFolder",
        "Folder",
        Kind::Choice(&[
            ("./images/", "./images/ (in the package)"),
            ("/media/images/", "/media/images/ (SPX)")
        ]),
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Image slot",
        "imageExtension",
        "Extension",
        Kind::Text,
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Image slot",
        "imageDefaultValue",
        "Default file",
        Kind::Text,
        Item,
        None,
        Layer
    ),
    p!(
        Look,
        "Image slot",
        "imageFit",
        "Fit",
        Kind::Choice(&[("cover", "Cover"), ("contain", "Contain")]),
        Item,
        None,
        Layer
    ),
    // Chart
    p!(
        Chart,
        "Chart",
        "chartKind",
        "Type",
        Kind::Choice(&[("bar", "Bar"), ("pie", "Pie"), ("line", "Line")]),
        Item,
        None,
        Layer
    ),
    p!(
        Chart,
        "Chart",
        "chartValueSort",
        "Sort by value",
        Kind::Choice(&[
            ("none", "List order"),
            ("asc", "Rising"),
            ("desc", "Falling")
        ]),
        Item,
        None,
        Layer
    ),
    p!(
        Chart,
        "Chart",
        "chartReveal",
        "Reveal",
        num(0.0, 1.0, 0.01),
        Item,
        Some("chartReveal"),
        Pose
    ),
    p!(
        Chart,
        "Style",
        "fontFamily",
        "Font",
        Kind::Font,
        Item,
        None,
        Layer
    ),
    p!(
        Chart,
        "Style",
        "fontSize",
        "Font size",
        num(6.0, 200.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Chart,
        "Style",
        "textColor",
        "Text color",
        Kind::Color,
        Item,
        None,
        Layer
    ),
    p!(
        Chart,
        "Style",
        "chartShowLegend",
        "Show legend",
        Kind::Toggle,
        Item,
        None,
        Layer
    ),
    p!(
        Chart,
        "Style",
        "chartCutout",
        "Hole %",
        num(0.0, 80.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Chart,
        "Style",
        "chartBarThickness",
        "Bar thickness %",
        num(10.0, 100.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Chart,
        "Style",
        "chartShowValues",
        "Show values",
        Kind::Toggle,
        Item,
        None,
        Layer
    ),
    p!(
        Chart,
        "Style",
        "chartLineWidth",
        "Line width",
        num(1.0, 16.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Chart,
        "Style",
        "chartPointRadius",
        "Point size",
        num(0.0, 20.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Chart,
        "Style",
        "chartLineTension",
        "Curve",
        num(0.0, 1.0, 0.05),
        Item,
        None,
        Layer
    ),
    p!(
        Chart,
        "Data points",
        "chartPoints",
        "Points",
        Kind::ChartPoints,
        Item,
        None,
        Layer
    ),
    // QR code
    p!(
        Qr,
        "QR code",
        "qrContent",
        "Content",
        Kind::TextArea,
        Item,
        None,
        Layer
    ),
    p!(
        Qr,
        "QR code",
        "qrVariableId",
        "Variable ID",
        Kind::Text,
        Item,
        None,
        Rebuild
    ),
    p!(
        Qr,
        "QR code",
        "qrFgColor",
        "Color",
        Kind::Color,
        Item,
        None,
        Layer
    ),
    p!(
        Qr,
        "QR code",
        "qrBgColor",
        "Background",
        Kind::Color,
        Item,
        None,
        Layer
    ),
    p!(
        Qr,
        "QR code",
        "qrErrorCorrection",
        "Error correction",
        Kind::Choice(&[
            ("L", "L (7 %)"),
            ("M", "M (15 %)"),
            ("Q", "Q (25 %)"),
            ("H", "H (30 %)")
        ]),
        Item,
        None,
        Layer
    ),
    p!(
        Qr,
        "QR code",
        "qrMargin",
        "Margin",
        num(0.0, 8.0, 1.0),
        Item,
        None,
        Layer
    ),
    // Particles
    p!(
        Particles,
        "Emitter",
        "particleEmitting",
        "Emitting",
        Kind::Toggle,
        Item,
        Some("particleEmitting"),
        Pose
    ),
    p!(
        Particles,
        "Emitter",
        "particleSpawnRate",
        "Spawn / sec",
        num(0.0, 10_000.0, 1.0),
        Item,
        Some("particleSpawnRate"),
        Pose
    ),
    p!(
        Particles,
        "Emitter",
        "particleEmitterX",
        "Emitter X",
        num(-ANY, ANY, 1.0),
        Item,
        Some("particleEmitterX"),
        Pose
    ),
    p!(
        Particles,
        "Emitter",
        "particleEmitterY",
        "Emitter Y",
        num(-ANY, ANY, 1.0),
        Item,
        Some("particleEmitterY"),
        Pose
    ),
    p!(
        Particles,
        "Emitter",
        "particleEmitterShape",
        "Shape",
        Kind::Choice(&[("circle", "Circle"), ("box", "Box")]),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Emitter",
        "particleEmitterRadius",
        "Radius",
        num(0.0, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Emitter",
        "particleEmitterWidth",
        "Width",
        num(0.0, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Emitter",
        "particleEmitterHeight",
        "Height",
        num(0.0, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Emitter",
        "particleEmitAngleDeg",
        "Angle °",
        num(-ANY, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Emitter",
        "particleAttach",
        "Space",
        Kind::Choice(&[
            ("absolute", "Absolute"),
            ("relative", "Relative to emitter")
        ]),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Emitter",
        "particleMaxCount",
        "Max count",
        num(1.0, 100_000.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Emitter",
        "particleLifetimeMs",
        "Lifetime (ms)",
        num(1.0, ANY, 50.0),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Motion",
        "particleSpeed",
        "Speed",
        num(0.0, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Motion",
        "particleSpreadDeg",
        "Spread °",
        num(0.0, 360.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Motion",
        "particleGravityX",
        "Gravity X",
        num(-ANY, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Motion",
        "particleGravityY",
        "Gravity Y",
        num(-ANY, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Look",
        "particleRotationDeg",
        "Rotation °",
        num(-ANY, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Look",
        "particleSpinDeg",
        "Spin ° / s",
        num(-ANY, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Look",
        "particleRotationRandomDeg",
        "Random rotation °",
        num(0.0, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Look",
        "particleAlignToVelocity",
        "Align to velocity",
        Kind::Toggle,
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Look",
        "particleSize",
        "Size",
        num(0.5, ANY, 0.5),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Look",
        "particleSizeRamp",
        "Size over life",
        Kind::ValueRamp,
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Look",
        "particleColorMode",
        "Color mode",
        Kind::Choice(&[
            ("ramp", "Ramp over life"),
            ("paletteCycle", "Palette cycle")
        ]),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Look",
        "particleColorRamp",
        "Color over life",
        Kind::Ramp,
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Look",
        "particlePaletteColors",
        "Palette",
        Kind::ColorList,
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Look",
        "particleAlphaRamp",
        "Opacity over life",
        Kind::ValueRamp,
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Look",
        "particleBlend",
        "Blend",
        Kind::Choice(&[("additive", "Additive"), ("normal", "Normal")]),
        Item,
        None,
        Layer
    ),
    p!(
        Particles,
        "Look",
        "particleSpriteUrl",
        "Sprite",
        Kind::Media("images"),
        Item,
        None,
        Rebuild
    ),
    p!(
        Particles,
        "Look",
        "particleSeed",
        "Seed",
        Kind::Seed,
        Item,
        None,
        Layer
    ),
    // Lightning
    p!(
        Lightning,
        "Bolt",
        "lightningEmitting",
        "Emitting",
        Kind::Toggle,
        Item,
        Some("lightningEmitting"),
        Pose
    ),
    p!(
        Lightning,
        "Bolt",
        "lightningAx",
        "A X",
        num(-ANY, ANY, 1.0),
        Item,
        Some("lightningAx"),
        Pose
    ),
    p!(
        Lightning,
        "Bolt",
        "lightningAy",
        "A Y",
        num(-ANY, ANY, 1.0),
        Item,
        Some("lightningAy"),
        Pose
    ),
    p!(
        Lightning,
        "Bolt",
        "lightningBx",
        "B X",
        num(-ANY, ANY, 1.0),
        Item,
        Some("lightningBx"),
        Pose
    ),
    p!(
        Lightning,
        "Bolt",
        "lightningBy",
        "B Y",
        num(-ANY, ANY, 1.0),
        Item,
        Some("lightningBy"),
        Pose
    ),
    p!(
        Lightning,
        "Bolt",
        "lightningSegments",
        "Segments",
        num(2.0, 48.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Lightning,
        "Bolt",
        "lightningAmplitude",
        "Amplitude",
        num(0.0, ANY, 0.5),
        Item,
        None,
        Layer
    ),
    p!(
        Lightning,
        "Bolt",
        "lightningBranches",
        "Branches",
        num(0.0, 6.0, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Lightning,
        "Bolt",
        "lightningFlickerMs",
        "Flicker (ms)",
        num(8.0, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Lightning,
        "Look",
        "lightningWidth",
        "Width",
        num(0.5, ANY, 0.25),
        Item,
        None,
        Layer
    ),
    p!(
        Lightning,
        "Look",
        "lightningGlowWidth",
        "Glow width",
        num(0.0, ANY, 0.5),
        Item,
        None,
        Layer
    ),
    p!(
        Lightning,
        "Look",
        "lightningColor",
        "Color",
        Kind::Color,
        Item,
        None,
        Layer
    ),
    p!(
        Lightning,
        "Look",
        "lightningGlowColor",
        "Glow color",
        Kind::Color,
        Item,
        None,
        Layer
    ),
    p!(
        Lightning,
        "Look",
        "lightningSeed",
        "Seed",
        Kind::Seed,
        Item,
        None,
        Layer
    ),
    // Pattern
    p!(
        Pattern,
        "Pattern",
        "patternKind",
        "Preset",
        Kind::Choice(&[
            ("checker", "Checker"),
            ("stripes", "Stripes"),
            ("dots", "Dots"),
            ("grid", "Grid")
        ]),
        Item,
        None,
        Layer
    ),
    p!(
        Pattern,
        "Pattern",
        "patternPhase",
        "Phase (cycles)",
        num(-ANY, ANY, 0.01),
        Item,
        Some("patternPhase"),
        Pose
    ),
    p!(
        Pattern,
        "Pattern",
        "patternColorA",
        "Color A",
        Kind::Color,
        Item,
        None,
        Layer
    ),
    p!(
        Pattern,
        "Pattern",
        "patternColorB",
        "Color B",
        Kind::Color,
        Item,
        None,
        Layer
    ),
    p!(
        Pattern,
        "Pattern",
        "patternStripeWidth",
        "Stripe width",
        num(1.0, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Pattern,
        "Pattern",
        "patternStripeGap",
        "Gap",
        num(0.0, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Pattern,
        "Pattern",
        "patternStripeAngleDeg",
        "Angle °",
        num(-ANY, ANY, 0.1),
        Item,
        None,
        Layer
    ),
    p!(
        Pattern,
        "Pattern",
        "patternCellSize",
        "Cell size",
        num(2.0, ANY, 1.0),
        Item,
        None,
        Layer
    ),
    p!(
        Pattern,
        "Pattern",
        "patternDotRadius",
        "Dot radius",
        num(0.5, ANY, 0.5),
        Item,
        None,
        Layer
    ),
    p!(
        Pattern,
        "Pattern",
        "patternLineWidth",
        "Line width",
        num(0.5, ANY, 0.5),
        Item,
        None,
        Layer
    ),
    // Lottie and video clips
    p!(
        Lottie,
        "Lottie",
        "mediaUrl",
        "Source",
        Kind::Media("lottie"),
        Item,
        None,
        Rebuild
    ),
    p!(
        Lottie,
        "Lottie",
        "lottieStartMs",
        "Start (ms)",
        Kind::ClipStart,
        Item,
        None,
        Layer
    ),
    p!(
        Lottie,
        "Lottie",
        "lottieLoop",
        "Loop",
        Kind::Toggle,
        Item,
        None,
        Layer
    ),
    p!(
        Video,
        "Video (experimental)",
        "mediaUrl",
        "Source",
        Kind::Media("videos"),
        Item,
        None,
        Rebuild
    ),
    p!(
        Video,
        "Video (experimental)",
        "videoStartMs",
        "Start (ms)",
        Kind::ClipStart,
        Item,
        None,
        Layer
    ),
    p!(
        Video,
        "Video (experimental)",
        "videoLoop",
        "Loop",
        Kind::Toggle,
        Item,
        None,
        Layer
    ),
];

fn is_text(item: &Item) -> bool {
    item.item_type == Some(ItemType::Text)
}

/// Layers with FRAME's Look tab (fill, border, corners): all but Lottie,
/// video, empty groups and patterns.
/// FRAME's info text for a section title (the ⓘ next to it), if any.
/// `item` is the selected layer (some texts depend on its kind).
pub fn group_help(
    tab: Tab,
    group: &str,
    item: Option<&Item>,
    board_hook: bool,
) -> Option<&'static str> {
    let svg_tint =
        item.is_some_and(|i| i.item_type == Some(ItemType::Image) && i.svg_tint == Some(true));
    Some(match (tab, group) {
        (Tab::Scene, "Canvas") => "The viewport and exports use these dimensions immediately.",
        (Tab::Scene, "SPX template") => {
            "Metadata written into the SPX template JSON. Out can be a millisecond duration or the word manual. Prevent Toggle: play while already on-air does not stop the graphic."
        }
        (Tab::Scene, "BOARD") => "Obtain Magic ID from MENiAC's account page.",
        (Tab::Scene, "Plates") => {
            "Editor-only underlay plate behind your graphics. Never exported to OGraf / SPX / HTML."
        }
        (Tab::Scene, "Guides") => {
            "Editor-only overlays (Safe 10% / 20%, Thirds, Center +, Rulers): shown with the toggles above the viewport. Never exported."
        }
        (Tab::Scene, "Custom rulers") => {
            "Your own guide lines, in percent of the scene. Show them with Rulers above the viewport; moving layers snap to them when Snap is on."
        }
        (Tab::Scene, "Cues") => {
            "Edit on the timeline (Pause / API / Script diamonds). Select a diamond for time, delay and type fields."
        }
        (Tab::Transform, "Live visibility") => {
            "BOARD Hook key — match a BOARD bool variable or switch name (e.g. is_it_player_1_turn). Polls live JSON and shows/hides this layer. Enable Invert on the opposing arrow for the same bool."
        }
        (Tab::Look, "Image slot") => {
            "Editor shows a placeholder only. The image is chosen in the OGraf / Air host (or SPX filelist) at playout."
        }
        (Tab::Look, "Fill") if svg_tint => {
            "Fill is clipped to the SVG silhouette (same solid / gradient / image options as shapes)."
        }
        (Tab::Look, "Border") if svg_tint => {
            "Follows the SVG silhouette (not the bounding box), like polygon stroke."
        }
        (Tab::Look, "Border") => {
            "Timeline animates a uniform width (all sides). Enable the Border Width channel for keys. 4-sided mode is editor layout only."
        }
        (Tab::Text, "Text") if board_hook => {
            "Display text is the editor default. Variable ID is the OGraf / SPX field id (and the live JSON key when BOARD Hook is on)."
        }
        (Tab::Text, "Text") => {
            "Display text is the editor default. Variable ID is the OGraf / SPX field id."
        }
        (Tab::Chart, "Chart") => {
            "Reveal 0–1: pie sweeps; bar/line values grow. Scrub-safe (no Chart.js tweens). Values accept decimals (e.g. 33.7)."
        }
        (Tab::Chart, "Data points") => {
            "Values accept decimals (e.g. 33.7). With BOARD Hook on, optional Variable ID binds each point to a BOARD int/float counter or number key."
        }
        (Tab::Qr, "QR code") => {
            "Fills the largest square inside the item box. Keep a quiet zone for reliable LED/wall scans."
        }
        (Tab::Particles, "Emitter") => {
            "Simulation is deterministic from seed + timeline time. High rate × long lifetime costs more CPU. Absolute space: trail stays in place (enable Emitter X/Y and keyframe the path). Relative: whole cloud sticks to the emitter."
        }
        (Tab::Particles, "Look") => {
            "Built-in sprites are white on black — best with Additive blend."
        }
        (Tab::Lightning, "Bolt") => {
            "Seek-stable jagged bolt from A→B. Emitting is animatable (step/hold). Flicker reseed buckets by timeline time."
        }
        (Tab::Pattern, "Pattern") => {
            "Seamless canvas fill. Phase is in cycles (1 = one full period) and wraps — keyframe 0→2 for two scrolls. Seek-stable."
        }
        (Tab::Lottie, "Lottie") => {
            "Play-only clip. Drag the timeline Start key to set when the animation begins (one move-only key). Use Transform opacity/scale for ins and outs. Before start shows frame 0; without Loop, freezes on the last frame."
        }
        (Tab::Video, _) => {
            "Play-only clip. Drag the timeline Start key to set when the clip begins (one move-only key). Use Transform opacity/scale for ins and outs. Before start shows frame 0; without Loop, freezes on the last frame."
        }
        (Tab::Modifiers, _) => {
            "Modifiers apply in list order. Enable/disable with the checkbox. Hover the info icon on a modifier name for how that effect works."
        }
        _ => return None,
    })
}

/// Fields shown only while the BOARD Hook is on (as in FRAME).
pub fn needs_board_hook(path: &str) -> bool {
    matches!(
        path,
        "ionboardMagicId"
            | "ionboardProjectId"
            | "ionboardPanelEnabled"
            | "visibilityVariableId"
            | "visibilityInvert"
    )
}

/// FRAME's info text for a field, if it has one.
pub fn help(path: &str, board_hook: bool) -> Option<&'static str> {
    Some(match path {
        "ionboardHookEnabled" | "ionboardMagicId" => "Obtain Magic ID from MENiAC's account page.",
        "ionboardProjectId" => "Insert here the BOARD project ID.",
        "ionboardPanelEnabled" => {
            "Adds an SPX panel field (16:9) whose url is an absolute BOARD link (board.meniac.tech/spx, or /play/… when Project ID is set). Same Magic auth as BOARD Connect. For SPX Play Item wiring from the board, keep the Connect plugin."
        }
        "visibilityVariableId" => {
            "BOARD Hook key — match a BOARD bool variable or switch name (e.g. is_it_player_1_turn). Polls live JSON and shows/hides this layer. Enable Invert on the opposing arrow for the same bool."
        }
        "visibilityInvert" => {
            "Show this layer when the live value is false. Use on player-2 arrow with the same turn bool as player 1."
        }
        "textVariableId" if board_hook => {
            "OGraf / playout field id, and with BOARD Hook enabled also the live JSON key (match the BOARD component / variable name)."
        }
        "textVariableId" => "OGraf / SPX field id.",
        "qrVariableId" if board_hook => {
            "OGraf / playout field id, and with BOARD Hook enabled also the live JSON key that overrides Content at playout."
        }
        "qrVariableId" => "OGraf / SPX field id.",
        _ => return None,
    })
}

fn is_svg(item: &Item) -> bool {
    let url = item.media_url.as_deref().unwrap_or("");
    url.split(['?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .ends_with(".svg")
}

fn is_filled(item: &Item) -> bool {
    !matches!(
        item.item_type,
        Some(ItemType::Lottie | ItemType::Video | ItemType::Empty | ItemType::Pattern)
    )
}

fn pattern_kind(item: &Item) -> &str {
    match item.pattern_kind.as_deref() {
        Some(k @ ("stripes" | "dots" | "grid")) => k,
        _ => "checker",
    }
}

fn chart_kind(item: &Item) -> &str {
    match item.chart_kind.as_deref() {
        Some("pie") => "pie",
        Some("line") => "line",
        _ => "bar",
    }
}

/// The value at a dotted JSON path (`shadow.x`, `borderRadius.0`), or
/// `null`.
pub fn lookup(root: &Value, path: &str) -> Value {
    path.split('.')
        .try_fold(root, |v, part| match v {
            Value::Object(m) => m.get(part),
            Value::Array(a) => part.parse::<usize>().ok().and_then(|i| a.get(i)),
            _ => None,
        })
        .cloned()
        .unwrap_or(Value::Null)
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
            // FRAME: polygons and paths have their own stroke, no 4-sided mode.
            "borderWidthMode" => !matches!(shape, Some(ShapeType::Polygon | ShapeType::Path)),
            // In 4-sided mode the sides are the width.
            "borderWidth" => item.border_width_mode.as_deref() != Some("sides"),
            p if p.starts_with("borderWidths.") => {
                item.border_width_mode.as_deref() == Some("sides")
                    && !matches!(shape, Some(ShapeType::Polygon | ShapeType::Path))
            }
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
            p if p.starts_with("image") => item.item_type == Some(ItemType::ImageSlot),
            "chartShowLegend" | "chartCutout" => chart_kind(item) == "pie",
            "chartBarThickness" | "chartShowValues" => chart_kind(item) == "bar",
            "chartLineWidth" | "chartPointRadius" | "chartLineTension" => {
                chart_kind(item) == "line"
            }
            "particleEmitterWidth" | "particleEmitterHeight" => {
                item.particle_emitter_shape.as_deref() == Some("box")
            }
            "particleEmitterRadius" => item.particle_emitter_shape.as_deref() != Some("box"),
            "particlePaletteColors" => item.particle_color_mode.as_deref() == Some("paletteCycle"),
            "particleColorRamp" => item.particle_color_mode.as_deref() != Some("paletteCycle"),
            "patternStripeWidth" | "patternStripeGap" | "patternStripeAngleDeg" => {
                pattern_kind(item) == "stripes"
            }
            "patternCellSize" => pattern_kind(item) != "stripes",
            "patternDotRadius" => pattern_kind(item) == "dots",
            "patternLineWidth" => pattern_kind(item) == "grid",
            "polygonCurve" | "polygonPoints" => {
                matches!(shape, Some(ShapeType::Polygon | ShapeType::Path))
            }
            "mediaUrl" if self.tab == Tab::Look => item.item_type == Some(ItemType::Image),
            "svgTint" => item.item_type == Some(ItemType::Image) && is_svg(item),
            "svgToPolygon" => item.item_type == Some(ItemType::Image) && is_svg(item),
            _ => match self.tab {
                Tab::Look => is_filled(item),
                Tab::Text => is_text(item),
                Tab::Chart => item.item_type == Some(ItemType::Chart),
                Tab::Qr => item.item_type == Some(ItemType::Qr),
                Tab::Particles => item.item_type == Some(ItemType::Particles),
                Tab::Lightning => item.item_type == Some(ItemType::Lightning),
                Tab::Pattern => item.item_type == Some(ItemType::Pattern),
                Tab::Lottie => item.item_type == Some(ItemType::Lottie),
                Tab::Video => item.item_type == Some(ItemType::Video),
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
            (Scope::Item | Scope::Modifier, None) | (Scope::Modifier, _) => return Value::Null,
        }
        .unwrap_or(Value::Null);
        lookup(&root, self.path)
    }

    /// Typed text and toggles turned into the value stored.
    fn parse(&self, value: Value) -> Option<Value> {
        Some(match (self.kind, value) {
            (Kind::Number { min, max, .. }, Value::String(s)) => {
                Value::from(s.trim().parse::<f64>().ok()?.clamp(min, max))
            }
            (Kind::Number { min, max, .. }, Value::Number(n)) => {
                Value::from(n.as_f64()?.clamp(min, max))
            }
            (Kind::Time { min, max }, Value::Number(n)) => {
                Value::from(n.as_f64()?.round().clamp(min, max))
            }
            (Kind::Time { min, max }, Value::String(s)) => {
                Value::from(s.trim().parse::<f64>().ok()?.round().clamp(min, max))
            }
            (Kind::TextOrNumber, Value::String(s)) => match s.trim().parse::<f64>() {
                Ok(n) => Value::from(n),
                Err(_) => Value::String(s.trim().to_string()),
            },
            (Kind::Toggle, Value::Bool(b)) if self.lane.is_some() => {
                Value::from(f64::from(u8::from(b)))
            }
            (Kind::Seed, Value::String(s)) => {
                Value::from(s.trim().parse::<f64>().ok()?.round().max(1.0))
            }
            (Kind::Choice(_), Value::String(s)) if self.path == "fontWeight" => {
                Value::from(s.parse::<f64>().ok()?)
            }
            (_, v) => v,
        })
    }

    /// The command that sets this field of modifier `modifier` on `item`.
    pub fn modifier_command(
        &self,
        item: &str,
        modifier: &str,
        value: Value,
        time_ms: f64,
        auto_key: bool,
    ) -> Option<Box<dyn Command>> {
        let value = self.parse(value)?;
        if let Some(lane) = self.lane {
            return Some(Box::new(SetValueAt {
                item: item.into(),
                lane: lane.into(),
                time_ms,
                value: value.as_f64()?,
                auto_key,
            }));
        }
        let update = match self.kind {
            // Masks and path follows set up their source layer at start.
            Kind::MaskSource | Kind::PathSource => Update::Rebuild,
            _ => self.update,
        };
        Some(Box::new(SetField {
            target: FieldTarget::Modifier {
                item: item.into(),
                modifier: modifier.into(),
            },
            path: self.path.into(),
            value,
            update,
        }))
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
        let value = self.parse(value)?;
        if let (Some(id), true) = (item, matches!(self.path, "polygonCurve" | "polygonPoints")) {
            // Points: an array, or `{points, curve}` (Reset shape sets both).
            let (points, curve) = match (&value, self.path) {
                (Value::String(c), "polygonCurve") => (None, Some(c.clone())),
                (Value::Array(_), _) => (Some(value.clone()), None),
                (Value::Object(o), _) => (
                    o.get("points").cloned(),
                    o.get("curve").and_then(Value::as_str).map(str::to_string),
                ),
                _ => return None,
            };
            let points = match points {
                Some(p) => Some(serde_json::from_value(p).ok()?),
                None => None,
            };
            return Some(Box::new(crate::polygon::SetPolygon {
                item: id.into(),
                points,
                curve,
                to_shape: false,
            }));
        }
        if self.path == "ionboardHookEnabled" && value == Value::Bool(false) {
            // FRAME: switching the Hook off also removes the SPX panel.
            let set = |path: &str| -> Box<dyn Command> {
                Box::new(SetField {
                    target: FieldTarget::Scene,
                    path: path.into(),
                    value: Value::Bool(false),
                    update: Update::Rebuild,
                })
            };
            return Some(Box::new(crate::commands::Batch {
                label: "Turn off BOARD Hook".into(),
                commands: vec![set("ionboardHookEnabled"), set("ionboardPanelEnabled")],
            }));
        }
        if self.kind == Kind::SvgToPolygon {
            // Needs the SVG file: the app builds that command.
            return None;
        }
        if let (Kind::ClipStart, Some(id)) = (self.kind, item) {
            let t = match &value {
                Value::String(s) => s.trim().parse::<f64>().ok()?,
                v => v.as_f64()?,
            };
            return Some(Box::new(crate::keys::SetClipStart {
                item: id.into(),
                time_ms: t,
            }));
        }
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
            Scope::Modifier => return None,
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
        // Variants, so fields shown only for some settings are covered too.
        let mut box_particles = new_layer(NewLayer::Particles, None);
        box_particles.particle_emitter_shape = Some("box".into());
        box_particles.particle_color_mode = Some("paletteCycle".into());
        d.items.push(box_particles);
        let mut sided = new_layer(NewLayer::Box, None);
        sided.border_width_mode = Some("sides".into());
        sided.border_widths = Some([1.0, 2.0, 3.0, 4.0]);
        d.items.push(sided);
        for kind in ["stripes", "dots", "grid"] {
            let mut pattern = new_layer(NewLayer::Pattern, None);
            pattern.pattern_kind = Some(kind.into());
            d.items.push(pattern);
        }
        let svg = br#"<svg viewBox="0 0 10 10"><path d="M0 0 L10 0 L5 10 Z"/></svg>"#;
        d.items.push(crate::layers::new_svg_layer(
            "/media/images/s.svg",
            "S",
            svg,
            crate::layers::SvgMode::Tint,
            256.0,
        ));
        let mut h = History::default();
        for p in PROPERTIES {
            if p.kind == Kind::SvgToPolygon {
                assert!(p.applies(d.items.last()));
                continue;
            }
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
                    (Kind::ValueRamp, _) => {
                        serde_json::json!([{ "offset": 0, "value": 1 }, { "offset": 1, "value": 0 }])
                    }
                    (Kind::ColorList, _) => serde_json::json!(["#ff0000"]),
                    (Kind::Rulers, _) => serde_json::json!([50]),
                    (Kind::Seed, _) => Value::from(7),
                    (Kind::ClipStart, _) => Value::from(0),
                    (Kind::ChartPoints, _) => {
                        serde_json::json!([{ "id": "p", "label": "A", "value": 1 }])
                    }
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
        let d = doc_with(&[
            NewLayer::Text,
            NewLayer::Box,
            NewLayer::Empty,
            NewLayer::PieChart,
            NewLayer::Qr,
        ]);
        let tabs: Vec<Vec<Tab>> = d.items.iter().map(Tab::for_item).collect();
        assert_eq!(
            tabs,
            vec![
                vec![Tab::Transform, Tab::Look, Tab::Text, Tab::Modifiers],
                vec![Tab::Transform, Tab::Look, Tab::Modifiers],
                vec![Tab::Transform, Tab::Modifiers],
                vec![Tab::Transform, Tab::Look, Tab::Chart, Tab::Modifiers],
                vec![Tab::Transform, Tab::Look, Tab::Qr, Tab::Modifiers],
            ]
        );
        // Chart style fields follow the chart type.
        let pie = &d.items[3];
        let shown: Vec<&str> = Property::of(Tab::Chart)
            .filter(|p| p.applies(Some(pie)))
            .map(|p| p.path)
            .collect();
        assert!(shown.contains(&"chartCutout") && !shown.contains(&"chartBarThickness"));
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

    #[test]
    fn board_hook_off_also_removes_the_panel() {
        let mut d = doc_with(&[]);
        d.scene.ionboard_hook_enabled = Some(true);
        d.scene.ionboard_panel_enabled = Some(true);
        let hook = Property::of(Tab::Scene)
            .find(|p| p.path == "ionboardHookEnabled")
            .unwrap();
        let cmd = hook.command(None, Value::Bool(false), 0.0, false).unwrap();
        let mut h = History::default();
        h.execute(&mut d, cmd.as_ref()).unwrap();
        assert_eq!(
            (
                d.scene.ionboard_hook_enabled,
                d.scene.ionboard_panel_enabled
            ),
            (Some(false), Some(false))
        );
        h.undo(&mut d).unwrap();
        assert_eq!(d.scene.ionboard_panel_enabled, Some(true));
        assert!(needs_board_hook("visibilityInvert") && !needs_board_hook("ionboardHookEnabled"));
        assert!(help("ionboardProjectId", false).is_some());
    }
}
