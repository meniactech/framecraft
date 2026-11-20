// SPDX-License-Identifier: AGPL-3.0-or-later
//! Photoshop (PSD) import: reads an 8-bit RGB document's layers — bounds,
//! opacity, visibility, blend mode, names, groups and pixels (raw or RLE) —
//! so each one can become an image layer. Written for FrameCraft from
//! Adobe's published file format description; PSB, 16/32-bit and CMYK
//! documents are refused with a message. Text layers also bring their text
//! and style (font, size, colour, alignment) from the layer's `TySh` block.

/// One entry of a PSD's layer list, bottom to top.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// A pixel layer: its pixels as a PNG (`None` when empty).
    Layer(Layer),
    /// Start of a group (Photoshop's hidden "</Layer group>" divider; the
    /// group's own record, with its name, comes after its children).
    GroupEnd,
    /// A group's record, closing the group opened by the last `GroupEnd`.
    Group(Layer),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: String,
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    /// 0–1.
    pub opacity: f64,
    pub visible: bool,
    /// CSS blend mode (`normal`, `multiply`, …).
    pub blend: &'static str,
    pub png: Option<Vec<u8>>,
    /// A text layer's text and style (its pixels are in `png` too).
    pub text: Option<Text>,
}

/// A Photoshop text layer's text and its first style.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub content: String,
    /// PostScript font name, e.g. `MouseMemoirs-Regular`.
    pub font: String,
    /// Size in document pixels (Photoshop's size × the layer's scale).
    pub size: f64,
    /// CSS colour.
    pub color: String,
    /// `left`, `center` or `right`.
    pub align: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Psd {
    pub width: u32,
    pub height: u32,
    pub entries: Vec<Entry>,
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.b.len());
        let end = end.ok_or("The PSD file ends too early")?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        let s = self.take(2)?;
        Ok(u16::from_be_bytes([s[0], s[1]]))
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(self.u16()? as i16)
    }
    fn u32(&mut self) -> Result<u32, String> {
        let s = self.take(4)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }
    fn f64(&mut self) -> Result<f64, String> {
        let s = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(f64::from_be_bytes(a))
    }
    fn skip_block(&mut self) -> Result<(), String> {
        let n = self.u32()? as usize;
        self.take(n).map(|_| ())
    }
}

fn blend(key: &[u8]) -> &'static str {
    match key {
        b"mul " => "multiply",
        b"scrn" => "screen",
        b"over" => "overlay",
        b"dark" => "darken",
        b"lite" => "lighten",
        b"div " => "color-dodge",
        b"idiv" => "color-burn",
        b"hLit" => "hard-light",
        b"sLit" => "soft-light",
        b"diff" => "difference",
        b"smud" => "exclusion",
        b"hue " => "hue",
        b"sat " => "saturation",
        b"colr" => "color",
        b"lum " => "luminosity",
        _ => "normal",
    }
}

/// PackBits (Photoshop's RLE) into exactly `len` bytes.
fn unpack_bits(src: &[u8], len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut i = 0;
    while i < src.len() && out.len() < len {
        let n = src[i] as i8;
        i += 1;
        if n >= 0 {
            let count = n as usize + 1;
            let end = (i + count).min(src.len());
            out.extend_from_slice(&src[i..end]);
            i = end;
        } else if n != -128 {
            let count = 1 - n as isize;
            if let Some(&v) = src.get(i) {
                out.extend(std::iter::repeat_n(v, count as usize));
            }
            i += 1;
        }
    }
    out.resize(len, 0);
    out
}

// ---- Text layers: Photoshop's descriptor and EngineData formats ----

/// A value in a Photoshop descriptor (only the kinds text layers use are
/// kept; others are skipped).
#[derive(Debug, Clone, PartialEq)]
enum DVal {
    Str(String),
    Raw(Vec<u8>),
    Desc(Vec<(String, DVal)>),
    Other,
}

fn read_unicode(r: &mut Reader) -> Result<String, String> {
    let n = r.u32()? as usize;
    let units: Vec<u16> = r
        .take(n.checked_mul(2).ok_or("bad string")?)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_be_bytes(*c))
        .collect();
    Ok(String::from_utf16_lossy(&units)
        .trim_end_matches('\0')
        .to_string())
}

fn read_key(r: &mut Reader) -> Result<String, String> {
    let n = r.u32()? as usize;
    let len = if n == 0 { 4 } else { n };
    Ok(String::from_utf8_lossy(r.take(len)?).into_owned())
}

fn read_value(r: &mut Reader, kind: &[u8]) -> Result<DVal, String> {
    Ok(match kind {
        b"Objc" | b"GlbO" => DVal::Desc(read_descriptor(r)?),
        b"VlLs" => {
            let n = r.u32()?;
            for _ in 0..n {
                let k = r.take(4)?.to_vec();
                read_value(r, &k)?;
            }
            DVal::Other
        }
        b"doub" => {
            r.f64()?;
            DVal::Other
        }
        b"UntF" => {
            r.take(4)?;
            r.f64()?;
            DVal::Other
        }
        b"TEXT" => DVal::Str(read_unicode(r)?),
        b"enum" => {
            read_key(r)?;
            read_key(r)?;
            DVal::Other
        }
        b"long" => {
            r.u32()?;
            DVal::Other
        }
        b"comp" => {
            r.take(8)?;
            DVal::Other
        }
        b"bool" => {
            r.u8()?;
            DVal::Other
        }
        b"type" | b"GlbC" => {
            read_unicode(r)?;
            read_key(r)?;
            DVal::Other
        }
        b"alis" => {
            let n = r.u32()? as usize;
            r.take(n)?;
            DVal::Other
        }
        b"tdta" => {
            let n = r.u32()? as usize;
            DVal::Raw(r.take(n)?.to_vec())
        }
        b"UnFl" => {
            r.take(4)?;
            let n = r.u32()? as usize;
            r.take(n * 8)?;
            DVal::Other
        }
        k => return Err(format!("descriptor value {:?}", String::from_utf8_lossy(k))),
    })
}

fn read_descriptor(r: &mut Reader) -> Result<Vec<(String, DVal)>, String> {
    read_unicode(r)?;
    read_key(r)?;
    let n = r.u32()?;
    let mut out = Vec::new();
    for _ in 0..n {
        let key = read_key(r)?;
        let kind = r.take(4)?.to_vec();
        out.push((key, read_value(r, &kind)?));
    }
    Ok(out)
}

/// A value in EngineData, Photoshop's PostScript-like text description.
#[derive(Debug, Clone, PartialEq)]
enum EVal {
    Dict(Vec<(String, EVal)>),
    Arr(Vec<EVal>),
    Num(f64),
    Bool(bool),
    Str(String),
    Name(String),
}

impl EVal {
    fn get(&self, path: &[&str]) -> Option<&EVal> {
        let mut v = self;
        for p in path {
            v = match v {
                EVal::Dict(d) => &d.iter().find(|(k, _)| k == p)?.1,
                EVal::Arr(a) => a.get(p.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        Some(v)
    }
    fn num(&self) -> Option<f64> {
        match self {
            EVal::Num(n) => Some(*n),
            _ => None,
        }
    }
}

struct Engine<'a> {
    b: &'a [u8],
    at: usize,
}

impl Engine<'_> {
    fn skip_space(&mut self) {
        while self.at < self.b.len() && self.b[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }
    fn is_delim(c: u8) -> bool {
        c.is_ascii_whitespace() || b"/[]<>()".contains(&c)
    }
    fn value(&mut self, depth: usize) -> Option<EVal> {
        if depth > 64 {
            return None;
        }
        self.skip_space();
        let rest = &self.b[self.at..];
        if rest.starts_with(b"<<") {
            self.at += 2;
            let mut d = Vec::new();
            loop {
                self.skip_space();
                if self.b[self.at..].starts_with(b">>") {
                    self.at += 2;
                    return Some(EVal::Dict(d));
                }
                let EVal::Name(k) = self.value(depth + 1)? else {
                    return None;
                };
                let v = self.value(depth + 1)?;
                d.push((k, v));
            }
        }
        match *rest.first()? {
            b'[' => {
                self.at += 1;
                let mut a = Vec::new();
                loop {
                    self.skip_space();
                    if self.b.get(self.at) == Some(&b']') {
                        self.at += 1;
                        return Some(EVal::Arr(a));
                    }
                    a.push(self.value(depth + 1)?);
                }
            }
            b'(' => {
                self.at += 1;
                let mut raw = Vec::new();
                while let Some(&c) = self.b.get(self.at) {
                    self.at += 1;
                    match c {
                        b'\\' => {
                            if let Some(&n) = self.b.get(self.at) {
                                raw.push(n);
                                self.at += 1;
                            }
                        }
                        b')' => break,
                        _ => raw.push(c),
                    }
                }
                let s = if raw.starts_with(&[0xFE, 0xFF]) {
                    let units: Vec<u16> = raw[2..]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| u16::from_be_bytes(*c))
                        .collect();
                    String::from_utf16_lossy(&units)
                } else {
                    raw.iter().map(|&b| b as char).collect()
                };
                Some(EVal::Str(s))
            }
            b'/' => {
                self.at += 1;
                let start = self.at;
                while self.at < self.b.len() && !Self::is_delim(self.b[self.at]) {
                    self.at += 1;
                }
                Some(EVal::Name(
                    String::from_utf8_lossy(&self.b[start..self.at]).into_owned(),
                ))
            }
            _ => {
                let start = self.at;
                while self.at < self.b.len() && !Self::is_delim(self.b[self.at]) {
                    self.at += 1;
                }
                let word = std::str::from_utf8(&self.b[start..self.at]).ok()?;
                match word {
                    "true" => Some(EVal::Bool(true)),
                    "false" => Some(EVal::Bool(false)),
                    w => w.parse::<f64>().ok().map(EVal::Num),
                }
            }
        }
    }
}

fn parse_engine(b: &[u8]) -> Option<EVal> {
    Engine { b, at: 0 }.value(0)
}

/// A text layer's `TySh` block: transform, text descriptor (the text and
/// EngineData), then warp and bounds (not needed).
fn read_type_tool(data: &[u8]) -> Option<Text> {
    let mut r = Reader { b: data, at: 0 };
    r.u16().ok()?; // version
    let t: Vec<f64> = (0..6).map(|_| r.f64()).collect::<Result<_, _>>().ok()?;
    r.u16().ok()?; // text version
    r.u32().ok()?; // descriptor version
    let desc = read_descriptor(&mut r).ok()?;
    let find = |k: &str| desc.iter().find(|(key, _)| key == k).map(|(_, v)| v);
    let engine = match find("EngineData") {
        Some(DVal::Raw(b)) => parse_engine(b),
        _ => None,
    };
    let content = match find("Txt ") {
        Some(DVal::Str(s)) => s.clone(),
        _ => match engine.as_ref()?.get(&["EngineDict", "Editor", "Text"]) {
            Some(EVal::Str(s)) => s.clone(),
            _ => return None,
        },
    };
    // Photoshop ends paragraphs with \r.
    let content = content.trim_end_matches(['\r', '\n']).replace('\r', "\n");
    let e = engine.as_ref();
    // The first style run, with the document's default style under it.
    let style = e.and_then(|e| {
        e.get(&[
            "EngineDict",
            "StyleRun",
            "RunArray",
            "0",
            "StyleSheet",
            "StyleSheetData",
        ])
    });
    let default = e.and_then(|e| e.get(&["ResourceDict", "StyleSheetSet", "0", "StyleSheetData"]));
    let prop = |k: &str| {
        style
            .and_then(|s| s.get(&[k]))
            .or_else(|| default.and_then(|d| d.get(&[k])))
    };
    let font = prop("Font")
        .and_then(EVal::num)
        .and_then(|i| e?.get(&["ResourceDict", "FontSet", &(i as usize).to_string(), "Name"]))
        .and_then(|n| match n {
            EVal::Str(s) => Some(s.trim_end_matches('\0').to_string()),
            _ => None,
        })
        .unwrap_or_default();
    // Scale of the layer's transform (Photoshop sizes are before it).
    let scale = (t[0] * t[3] - t[1] * t[2]).abs().sqrt();
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let size = prop("FontSize").and_then(EVal::num).unwrap_or(12.0) * scale;
    let color = prop("FillColor")
        .and_then(|c| c.get(&["Values"]))
        .and_then(|v| match v {
            EVal::Arr(a) if a.len() == 4 => {
                let n: Vec<f64> = a.iter().filter_map(EVal::num).collect();
                (n.len() == 4).then_some(n)
            }
            _ => None,
        })
        .map_or("#000000".to_string(), |v| {
            let c = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
            if v[0] >= 0.999 {
                format!("#{:02x}{:02x}{:02x}", c(v[1]), c(v[2]), c(v[3]))
            } else {
                format!(
                    "rgba({}, {}, {}, {})",
                    c(v[1]),
                    c(v[2]),
                    c(v[3]),
                    (v[0] * 1000.0).round() / 1000.0
                )
            }
        });
    let align = e
        .and_then(|e| {
            e.get(&[
                "EngineDict",
                "ParagraphRun",
                "RunArray",
                "0",
                "ParagraphSheet",
                "Properties",
                "Justification",
            ])
        })
        .and_then(EVal::num)
        .map_or("left", |j| match j as i64 {
            1 => "right",
            2 => "center",
            _ => "left",
        });
    Some(Text {
        content,
        font,
        size,
        color,
        align,
    })
}

/// A PostScript font name as a family and CSS weight:
/// `MouseMemoirs-Regular` → ("Mouse Memoirs", 400), `Montserrat-SemiBold`
/// → ("Montserrat", 600).
pub fn font_from_postscript(ps: &str) -> (String, f64) {
    let (base, style) = ps.split_once('-').unwrap_or((ps, ""));
    let st = style.to_ascii_lowercase();
    let weight = [
        ("thin", 100.0),
        ("extralight", 200.0),
        ("ultralight", 200.0),
        ("semibold", 600.0),
        ("demibold", 600.0),
        ("extrabold", 800.0),
        ("ultrabold", 800.0),
        ("black", 900.0),
        ("heavy", 900.0),
        ("bold", 700.0),
        ("light", 300.0),
        ("medium", 500.0),
    ]
    .iter()
    .find(|(k, _)| st.contains(k))
    .map_or(400.0, |(_, w)| *w);
    // CamelCase to words: "OpenSans" → "Open Sans".
    let mut family = String::new();
    let mut prev_lower = false;
    for ch in base.chars() {
        if ch.is_uppercase() && prev_lower {
            family.push(' ');
        }
        prev_lower = ch.is_lowercase() || ch.is_ascii_digit();
        family.push(ch);
    }
    (family, weight)
}

struct Record {
    layer: Layer,
    channels: Vec<(i16, u32)>,
    divider: u32,
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().ok()?;
        w.write_image_data(rgba).ok()?;
    }
    Some(out)
}

/// Reads a PSD's layers.
pub fn parse(bytes: &[u8]) -> Result<Psd, String> {
    let mut r = Reader { b: bytes, at: 0 };
    if r.take(4)? != b"8BPS" {
        return Err("Not a Photoshop (PSD) file".into());
    }
    match r.u16()? {
        1 => {}
        2 => return Err("Large documents (PSB) are not supported yet".into()),
        v => return Err(format!("Unknown PSD version {v}")),
    }
    r.take(6)?;
    let _channels = r.u16()?;
    let height = r.u32()?;
    let width = r.u32()?;
    let depth = r.u16()?;
    let mode = r.u16()?;
    if depth != 8 {
        return Err(format!(
            "{depth}-bit PSD documents are not supported yet (save as 8 bits per channel)"
        ));
    }
    if mode != 3 {
        return Err(
            "Only RGB PSD documents are supported (convert CMYK / grayscale to RGB)".into(),
        );
    }
    r.skip_block()?; // color mode data
    r.skip_block()?; // image resources
    let lm_len = r.u32()? as usize;
    if lm_len == 0 {
        return Ok(Psd {
            width,
            height,
            entries: Vec::new(),
        });
    }
    let li_len = r.u32()? as usize;
    if li_len == 0 {
        return Ok(Psd {
            width,
            height,
            entries: Vec::new(),
        });
    }
    let count = r.i16()?.unsigned_abs() as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let top = r.i32()?;
        let left = r.i32()?;
        let bottom = r.i32()?;
        let right = r.i32()?;
        let n = r.u16()? as usize;
        let mut channels = Vec::with_capacity(n);
        for _ in 0..n {
            let id = r.i16()?;
            let len = r.u32()?;
            channels.push((id, len));
        }
        if r.take(4)? != b"8BIM" {
            return Err("Unexpected data in a PSD layer record".into());
        }
        let key = r.take(4)?;
        let opacity = f64::from(r.u8()?) / 255.0;
        let _clipping = r.u8()?;
        let flags = r.u8()?;
        let _filler = r.u8()?;
        let extra_len = r.u32()? as usize;
        let extra_end = r.at + extra_len;
        r.skip_block()?; // layer mask
        r.skip_block()?; // blending ranges
        let name_len = r.u8()? as usize;
        let mut name = String::from_utf8_lossy(r.take(name_len)?).into_owned();
        // Pascal string padded to a multiple of 4 (length byte included).
        let pad = (4 - (name_len + 1) % 4) % 4;
        r.take(pad)?;
        let mut divider = 0;
        let mut text = None;
        while r.at + 12 <= extra_end {
            let sig = r.take(4)?;
            if sig != b"8BIM" && sig != b"8B64" {
                break;
            }
            let k = r.take(4)?;
            let len = r.u32()? as usize;
            let data = r.take(len)?;
            match k {
                b"luni" if data.len() >= 4 => {
                    let n = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
                    let units: Vec<u16> = data[4..]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .take(n)
                        .map(|c| u16::from_be_bytes(*c))
                        .collect();
                    let s = String::from_utf16_lossy(&units);
                    let s = s.trim_end_matches('\0');
                    if !s.is_empty() {
                        name = s.to_string();
                    }
                }
                b"TySh" => text = read_type_tool(data),
                b"lsct" | b"lsdk" if data.len() >= 4 => {
                    divider = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                }
                _ => {}
            }
        }
        r.at = extra_end.min(bytes.len());
        records.push(Record {
            layer: Layer {
                name,
                left,
                top,
                width: (right - left).max(0) as u32,
                height: (bottom - top).max(0) as u32,
                opacity,
                visible: flags & 0x02 == 0,
                blend: blend(key),
                png: None,
                text,
            },
            channels,
            divider,
        });
    }
    // Channel image data, layer by layer, channel by channel.
    let mut entries = Vec::with_capacity(records.len());
    for rec in records {
        let (w, h) = (rec.layer.width as usize, rec.layer.height as usize);
        let mut rgba = vec![0u8; w * h * 4];
        let mut has_alpha = false;
        for &(id, len) in &rec.channels {
            let block = r.take(len as usize)?;
            if block.len() < 2 || w == 0 || h == 0 {
                continue;
            }
            let slot = match id {
                0 => 0,
                1 => 1,
                2 => 2,
                -1 => {
                    has_alpha = true;
                    3
                }
                _ => continue, // masks: their own size; not imported
            };
            let compression = u16::from_be_bytes([block[0], block[1]]);
            let data = &block[2..];
            let plane = match compression {
                0 => {
                    let mut p = data.to_vec();
                    p.resize(w * h, 0);
                    p
                }
                1 => {
                    // Row byte counts, then PackBits rows.
                    let counts_len = h * 2;
                    if data.len() < counts_len {
                        continue;
                    }
                    let mut rows = Vec::with_capacity(w * h);
                    let mut at = counts_len;
                    for row in 0..h {
                        let n = u16::from_be_bytes([data[row * 2], data[row * 2 + 1]]) as usize;
                        let end = (at + n).min(data.len());
                        rows.extend(unpack_bits(&data[at..end], w));
                        at = end;
                    }
                    rows
                }
                _ => continue, // ZIP-compressed channels: not supported yet
            };
            for (i, v) in plane.iter().take(w * h).enumerate() {
                rgba[i * 4 + slot] = *v;
            }
        }
        if !has_alpha {
            for px in rgba.as_chunks_mut::<4>().0 {
                px[3] = 255;
            }
        }
        let mut layer = rec.layer;
        let empty = w == 0 || h == 0 || rgba.as_chunks::<4>().0.iter().all(|px| px[3] == 0);
        if !empty && rec.divider == 0 {
            layer.png = encode_png(w as u32, h as u32, &rgba);
        }
        entries.push(match rec.divider {
            1 | 2 => Entry::Group(layer),
            3 => Entry::GroupEnd,
            _ => Entry::Layer(layer),
        });
    }
    Ok(Psd {
        width,
        height,
        entries,
    })
}

/// The PSD as FrameCraft layers, in stacking order: one Empty named after
/// the file holding everything, an Empty per group, and an image layer per
/// pixel layer at its place (opacity, visibility and blend mode kept).
/// `import` stores a layer's PNG (file name, bytes) and returns its media
/// URL; layers it cannot store are left out.
///
/// Text layers become editable text layers (font matched against
/// `fonts`, the families FrameCraft has), with their Photoshop pixels kept
/// as a hidden image just below. Returns the layers and the font families
/// that were not found.
pub fn to_items(
    psd: &Psd,
    file_name: &str,
    fonts: &[String],
    mut import: impl FnMut(&str, Vec<u8>) -> Option<String>,
) -> (Vec<crate::model::Item>, Vec<String>) {
    use crate::layers::{NewLayer, new_layer, new_media_layer};
    let stem = file_name
        .rsplit_once('.')
        .map_or(file_name, |(s, _)| s)
        .trim()
        .to_string();
    let empty = |name: &str, parent: Option<&str>| {
        let mut e = new_layer(NewLayer::Empty, parent);
        e.name = name.to_string();
        e.x = 0.0;
        e.y = 0.0;
        e.width = f64::from(psd.width);
        e.height = f64::from(psd.height);
        e
    };
    let norm = |f: &str| {
        f.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let mut missing: Vec<String> = Vec::new();
    let mut items = vec![empty(
        if stem.is_empty() { "Photoshop" } else { &stem },
        None,
    )];
    let root = items[0].id.clone();
    // Open groups: their Empty's index in `items`.
    let mut open: Vec<usize> = Vec::new();
    for entry in &psd.entries {
        let parent = open.last().map_or(root.clone(), |&i| items[i].id.clone());
        match entry {
            // A group starts (from below): its Empty goes here, so it stacks
            // where the group does; name and look come with its record.
            Entry::GroupEnd => {
                items.push(empty("Group", Some(&parent)));
                open.push(items.len() - 1);
            }
            Entry::Group(g) => {
                if let Some(i) = open.pop() {
                    let e = &mut items[i];
                    e.name = g.name.clone();
                    e.opacity = g.opacity;
                    if !g.visible {
                        e.visible = Some(false);
                    }
                }
            }
            Entry::Layer(l) if l.text.is_some() => {
                let t = l.text.as_ref().expect("text layer");
                // The Photoshop pixels, hidden, for comparing.
                if let Some(png) = l.png.clone() {
                    let file = format!("{stem}-{}.png", l.name.replace(['/', '\\'], "-"));
                    if let Some(url) = import(&file, png) {
                        let mut img = new_media_layer(
                            "images",
                            &url,
                            &format!("{} (Photoshop pixels)", l.name),
                            Some((f64::from(l.width), f64::from(l.height))),
                            Some(&parent),
                        );
                        img.x = f64::from(l.left);
                        img.y = f64::from(l.top);
                        img.visible = Some(false);
                        items.push(img);
                    }
                }
                let (guess, weight) = font_from_postscript(&t.font);
                let family = fonts
                    .iter()
                    .find(|f| norm(f) == norm(&guess))
                    .cloned()
                    .unwrap_or_else(|| {
                        if !guess.is_empty() && !missing.contains(&guess) {
                            missing.push(guess.clone());
                        }
                        guess.clone()
                    });
                // The text box around Photoshop's glyph bounds: a line's
                // height, wide enough not to wrap, anchored by alignment.
                let (left, top) = (f64::from(l.left), f64::from(l.top));
                let (w, h) = (f64::from(l.width), f64::from(l.height));
                let lines = t.content.lines().count().max(1) as f64;
                let box_h = (t.size * 1.25 * lines).max(h);
                let box_w = w + t.size * 0.5;
                let x = match t.align {
                    "right" => left + w - box_w,
                    "center" => left + w / 2.0 - box_w / 2.0,
                    _ => left,
                };
                let mut it = new_layer(NewLayer::Text, Some(&parent));
                it.name = l.name.clone();
                it.text_content = Some(t.content.clone());
                it.font_family = Some(family);
                it.font_size = Some((t.size * 10.0).round() / 10.0);
                it.font_weight = Some(weight);
                it.text_color = Some(t.color.clone());
                it.text_align = Some(t.align.to_string());
                it.x = x.round();
                it.y = (top - (box_h - h) / 2.0).round();
                it.width = box_w.round();
                it.height = box_h.round();
                it.opacity = l.opacity;
                if !l.visible {
                    it.visible = Some(false);
                }
                if l.blend != "normal" {
                    it.blend_mode = Some(l.blend.to_string());
                }
                items.push(it);
            }
            Entry::Layer(l) => {
                let Some(png) = l.png.clone() else { continue };
                let file = format!("{stem}-{}.png", l.name.replace(['/', '\\'], "-"));
                let Some(url) = import(&file, png) else {
                    continue;
                };
                let mut it = new_media_layer(
                    "images",
                    &url,
                    &l.name,
                    Some((f64::from(l.width), f64::from(l.height))),
                    Some(&parent),
                );
                it.x = f64::from(l.left);
                it.y = f64::from(l.top);
                it.opacity = l.opacity;
                if !l.visible {
                    it.visible = Some(false);
                }
                if l.blend != "normal" {
                    it.blend_mode = Some(l.blend.to_string());
                }
                items.push(it);
            }
        }
    }
    (items, missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal PSD: 4×2 canvas, one 2×1 layer at (1, 1), red with alpha,
    /// RLE channels, inside a group.
    fn sample() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend(b"8BPS");
        b.extend(1u16.to_be_bytes());
        b.extend([0u8; 6]);
        b.extend(3u16.to_be_bytes());
        b.extend(2u32.to_be_bytes()); // height
        b.extend(4u32.to_be_bytes()); // width
        b.extend(8u16.to_be_bytes());
        b.extend(3u16.to_be_bytes());
        b.extend(0u32.to_be_bytes()); // color mode data
        b.extend(0u32.to_be_bytes()); // image resources
        let mut li = Vec::new();
        li.extend(3i16.to_be_bytes());
        // A channel block: compression 1 (RLE), one row: [count, data].
        let rle = |v: u8| {
            let mut c = Vec::new();
            c.extend(1u16.to_be_bytes());
            c.extend(2u16.to_be_bytes()); // row byte count
            c.extend([0xFF, v]); // PackBits: repeat v twice
            c
        };
        let record = |li: &mut Vec<u8>,
                      rect: [i32; 4],
                      chans: &[(i16, usize)],
                      name: &str,
                      divider: Option<u32>,
                      flags: u8| {
            for v in rect {
                li.extend(v.to_be_bytes());
            }
            li.extend((chans.len() as u16).to_be_bytes());
            for (id, len) in chans {
                li.extend(id.to_be_bytes());
                li.extend((*len as u32).to_be_bytes());
            }
            li.extend(b"8BIMnorm");
            li.extend([128u8, 0, flags, 0]);
            let mut extra = Vec::new();
            extra.extend(0u32.to_be_bytes());
            extra.extend(0u32.to_be_bytes());
            extra.push(name.len() as u8);
            extra.extend(name.as_bytes());
            while (extra.len() - 8) % 4 != 0 {
                extra.push(0);
            }
            if let Some(d) = divider {
                extra.extend(b"8BIMlsct");
                extra.extend(4u32.to_be_bytes());
                extra.extend(d.to_be_bytes());
            }
            li.extend((extra.len() as u32).to_be_bytes());
            li.extend(extra);
        };
        let empty = [(0i16, 2usize), (1, 2), (2, 2), (-1, 2)];
        record(&mut li, [0, 0, 0, 0], &empty, "</Layer group>", Some(3), 0);
        let full = [(0i16, 6usize), (1, 6), (2, 6), (-1, 6)];
        record(&mut li, [1, 1, 2, 3], &full, "Red", None, 0);
        record(&mut li, [0, 0, 0, 0], &empty, "Group", Some(1), 0x02);
        // Channel data: dividers (raw, nothing), the red layer (RLE).
        for _ in 0..4 {
            li.extend(0u16.to_be_bytes());
        }
        for v in [255u8, 0, 0, 200] {
            li.extend(rle(v));
        }
        for _ in 0..4 {
            li.extend(0u16.to_be_bytes());
        }
        let mut lm = Vec::new();
        lm.extend((li.len() as u32).to_be_bytes());
        lm.extend(li);
        b.extend((lm.len() as u32).to_be_bytes());
        b.extend(lm);
        b
    }

    #[test]
    fn reads_layers_groups_and_pixels() {
        let psd = parse(&sample()).unwrap();
        assert_eq!((psd.width, psd.height), (4, 2));
        assert_eq!(psd.entries.len(), 3);
        assert_eq!(psd.entries[0], Entry::GroupEnd);
        let Entry::Layer(l) = &psd.entries[1] else {
            panic!("layer")
        };
        assert_eq!(
            (l.name.as_str(), l.left, l.top, l.width, l.height),
            ("Red", 1, 1, 2, 1)
        );
        assert!((l.opacity - 128.0 / 255.0).abs() < 1e-9);
        assert!(l.visible && l.blend == "normal");
        assert!(l.png.as_ref().is_some_and(|p| p.starts_with(b"\x89PNG")));
        let Entry::Group(g) = &psd.entries[2] else {
            panic!("group")
        };
        assert_eq!(g.name, "Group");
        assert!(!g.visible);
    }

    #[test]
    fn becomes_layers_under_one_empty() {
        let psd = parse(&sample()).unwrap();
        let mut stored = Vec::new();
        let (items, missing) = to_items(&psd, "Poster.psd", &[], |name, png| {
            stored.push(name.to_string());
            assert!(png.starts_with(b"\x89PNG"));
            Some(format!("/media/images/{name}"))
        });
        assert_eq!(stored, vec!["Poster-Red.png"]);
        assert!(missing.is_empty());
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["Poster", "Group", "Red"]);
        let parent = |i: usize| items[i].parent_id.clone().flatten();
        assert_eq!(parent(0), None);
        assert_eq!(parent(1).as_deref(), Some(items[0].id.as_str()));
        assert_eq!(parent(2).as_deref(), Some(items[1].id.as_str()));
        assert_eq!(items[1].visible, Some(false));
        assert_eq!(
            (items[2].x, items[2].y, items[2].width, items[2].height),
            (1.0, 1.0, 2.0, 1.0)
        );
    }

    #[test]
    fn engine_data_and_font_names() {
        let e = parse_engine(
            b"<< /EngineDict << /StyleRun << /RunArray [ << /StyleSheet << /StyleSheetData << /Font 1 /FontSize 48.5 >> >> >> ] >> /Editor << /Text (\xfe\xff\x00H\x00i) >> >> /ResourceDict << /FontSet [ << /Name (AdobeInvisFont) >> << /Name (Inter-Bold) >> ] >> >>",
        )
        .unwrap();
        let size = e.get(&[
            "EngineDict",
            "StyleRun",
            "RunArray",
            "0",
            "StyleSheet",
            "StyleSheetData",
            "FontSize",
        ]);
        assert_eq!(size.and_then(EVal::num), Some(48.5));
        assert_eq!(
            e.get(&["EngineDict", "Editor", "Text"]),
            Some(&EVal::Str("Hi".into()))
        );
        assert_eq!(
            e.get(&["ResourceDict", "FontSet", "1", "Name"]),
            Some(&EVal::Str("Inter-Bold".into()))
        );
        assert_eq!(
            font_from_postscript("MouseMemoirs-Regular"),
            ("Mouse Memoirs".into(), 400.0)
        );
        assert_eq!(
            font_from_postscript("Inter-SemiBold"),
            ("Inter".into(), 600.0)
        );
        assert_eq!(
            font_from_postscript("OpenSans-ExtraBold"),
            ("Open Sans".into(), 800.0)
        );
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        assert!(parse(b"nope").is_err());
        let mut b = sample();
        b[4..6].copy_from_slice(&2u16.to_be_bytes());
        assert!(parse(&b).unwrap_err().contains("PSB"));
        assert_eq!(
            unpack_bits(&[0x02, 1, 2, 3, 0xFE, 9], 6),
            vec![1, 2, 3, 9, 9, 9]
        );
    }
}
