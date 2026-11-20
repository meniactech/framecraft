// SPDX-License-Identifier: AGPL-3.0-or-later
//! How times are shown and typed, as FRAME's `timecode.ts`: milliseconds,
//! seconds, mm:ss.mmm, hh:mm:ss.mmm, frames, or timecode hh:mm:ss:ff
//! (non-drop). Times are always stored in milliseconds.

/// A time display format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    Ms,
    /// FRAME's default: typing 30 for a 30 s scene is the common case.
    #[default]
    S,
    MinSec,
    HourMinSec,
    Frames,
    Timecode,
}

/// The formats in FRAME's order: `(format, id, label, title)`.
pub const FORMATS: &[(Format, &str, &str, &str)] = &[
    (Format::Ms, "ms", "ms", "Milliseconds"),
    (Format::S, "s", "s", "Seconds"),
    (
        Format::MinSec,
        "mm:ss.mmm",
        "mm:ss",
        "Minutes : seconds . milliseconds",
    ),
    (
        Format::HourMinSec,
        "hh:mm:ss.mmm",
        "hh:mm:ss",
        "Hours : minutes : seconds . milliseconds",
    ),
    (Format::Frames, "frames", "fr", "Frames (uses timeline FPS)"),
    (
        Format::Timecode,
        "hh:mm:ss:ff",
        "tc",
        "Timecode hh:mm:ss:ff (non-drop)",
    ),
];

impl Format {
    /// FRAME's id (also what is saved).
    pub fn id(self) -> &'static str {
        FORMATS.iter().find(|f| f.0 == self).map_or("s", |f| f.1)
    }

    /// The format with FRAME's id; seconds for anything else.
    pub fn from_id(id: &str) -> Self {
        FORMATS
            .iter()
            .find(|f| f.1 == id)
            .map_or(Format::S, |f| f.0)
    }

    /// Unit hint for labels and placeholders (FRAME's `timeFormatUnitHint`).
    pub fn hint(self) -> &'static str {
        match self {
            Format::Ms => "ms",
            Format::S => "s",
            Format::Frames => "fr",
            Format::MinSec => "mm:ss.mmm",
            Format::HourMinSec => "hh:mm:ss.mmm",
            Format::Timecode => "hh:mm:ss:ff",
        }
    }
}

fn rate(fps: f64) -> f64 {
    if fps.is_finite() {
        fps.round().max(1.0)
    } else {
        30.0
    }
}

/// Whole frames from ms (nearest).
pub fn ms_to_frames(ms: f64, fps: f64) -> f64 {
    (ms.max(0.0) / 1000.0 * rate(fps)).round().max(0.0)
}

pub fn frames_to_ms(frames: f64, fps: f64) -> f64 {
    (frames.max(0.0) / rate(fps) * 1000.0).round().max(0.0)
}

fn pad(n: f64, width: usize) -> String {
    format!("{:0width$}", n.max(0.0).floor() as u64)
}

/// `ms` shown in `format` (FRAME's `formatTimeMs`).
pub fn format_time(ms: f64, format: Format, fps: f64) -> String {
    let t = if ms.is_finite() { ms.max(0.0) } else { 0.0 };
    let r = rate(fps);
    match format {
        Format::Ms => return crate::js::number_to_string(t.round()),
        Format::S => {
            let sec = t / 1000.0;
            // Clean whole seconds (30, not 30.000) when exact.
            if (sec - sec.round()).abs() < 0.0005 {
                return crate::js::number_to_string(sec.round());
            }
            return crate::js::number_to_string((sec * 1000.0).round() / 1000.0);
        }
        Format::Frames => return crate::js::number_to_string(ms_to_frames(t, r)),
        _ => {}
    }
    let total = t.round();
    let hours = (total / 3_600_000.0).floor();
    let minutes = ((total % 3_600_000.0) / 60_000.0).floor();
    let seconds = ((total % 60_000.0) / 1000.0).floor();
    let millis = total % 1000.0;
    match format {
        Format::MinSec => format!(
            "{}:{}.{}",
            pad((total / 60_000.0).floor(), 2),
            pad(seconds, 2),
            pad(millis, 3)
        ),
        Format::HourMinSec => format!(
            "{}:{}:{}.{}",
            pad(hours, 2),
            pad(minutes, 2),
            pad(seconds, 2),
            pad(millis, 3)
        ),
        _ => {
            // hh:mm:ss:ff — the frame within the second (0 … fps-1).
            let frames = (total / 1000.0 * r).floor();
            let ff = frames % r;
            let ss = (frames / r).floor() % 60.0;
            let mm = (frames / r / 60.0).floor() % 60.0;
            let hh = (frames / r / 3600.0).floor();
            format!(
                "{}:{}:{}:{}",
                pad(hh, 2),
                pad(mm, 2),
                pad(ss, 2),
                pad(ff, 2)
            )
        }
    }
}

fn num(s: &str) -> Option<f64> {
    s.replace(',', "")
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
}

/// Typed text back to ms (FRAME's `parseTimeMs`). Plain numbers count in
/// the active format; a unit (`30s`, `500ms`, `90fr`) always wins.
pub fn parse_time(input: &str, format: Format, fps: f64) -> Option<f64> {
    let raw = input.trim();
    if raw.is_empty() {
        return None;
    }
    let r = rate(fps);
    // Explicit units.
    let lower = raw.to_ascii_lowercase();
    let split = lower
        .find(|c: char| c.is_ascii_alphabetic())
        .map(|i| lower.split_at(i));
    if let Some((n, unit)) = split {
        let unit = unit.trim();
        let ms_units = ["ms", "msec", "millisecond", "milliseconds"];
        let s_units = ["s", "sec", "secs", "second", "seconds"];
        let fr_units = ["fr", "frame", "frames"];
        if let Some(n) = num(n) {
            if ms_units.contains(&unit) {
                return Some(n.round().max(0.0));
            }
            if s_units.contains(&unit) {
                return Some((n * 1000.0).round().max(0.0));
            }
            if fr_units.contains(&unit) {
                return Some(frames_to_ms(n, r));
            }
        }
    }
    let plain_ms = || num(raw).map(|n| n.round().max(0.0));
    match format {
        Format::Ms => return plain_ms(),
        Format::S => return num(raw).map(|n| (n * 1000.0).round().max(0.0)),
        Format::Frames => return num(raw).map(|n| frames_to_ms(n, r)),
        _ => {}
    }
    let parts: Vec<&str> = raw.split([':', ';', '.']).collect();
    let all_digits = |p: &[&str]| {
        p.iter()
            .all(|x| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit()))
    };
    let millis = |m: &str| -> f64 {
        let s: String = format!("{m:0<3}").chars().take(3).collect();
        s.parse().unwrap_or(0.0)
    };
    let f = |s: &str| s.parse::<f64>().unwrap_or(0.0);
    match format {
        Format::MinSec => {
            if raw.contains(';') || !all_digits(&parts) || !(2..=3).contains(&parts.len()) {
                return plain_ms();
            }
            let ms = parts.get(2).map_or(0.0, |m| millis(m));
            Some((f(parts[0]) * 60_000.0 + f(parts[1]) * 1000.0 + ms).round())
        }
        Format::HourMinSec => {
            if raw.contains(';') || !all_digits(&parts) || !(3..=4).contains(&parts.len()) {
                return plain_ms();
            }
            let ms = parts.get(3).map_or(0.0, |m| millis(m));
            Some(
                (f(parts[0]) * 3_600_000.0 + f(parts[1]) * 60_000.0 + f(parts[2]) * 1000.0 + ms)
                    .round(),
            )
        }
        _ => {
            // hh:mm:ss:ff (';' drop-frame separators read as non-drop).
            let tc: Vec<&str> = raw.split([':', ';']).collect();
            if tc.len() == 4 && all_digits(&tc) {
                let frames =
                    ((f(tc[0]) * 60.0 + f(tc[1])) * 60.0 + f(tc[2])) * r + f(tc[3]).min(r - 1.0);
                return Some(frames_to_ms(frames, r));
            }
            plain_ms()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_as_frame() {
        assert_eq!(format_time(30000.0, Format::S, 25.0), "30");
        assert_eq!(format_time(1234.0, Format::S, 25.0), "1.234");
        assert_eq!(format_time(1234.4, Format::Ms, 25.0), "1234");
        assert_eq!(format_time(61_250.0, Format::MinSec, 25.0), "01:01.250");
        assert_eq!(
            format_time(3_661_005.0, Format::HourMinSec, 25.0),
            "01:01:01.005"
        );
        assert_eq!(format_time(2000.0, Format::Frames, 25.0), "50");
        assert_eq!(format_time(61_480.0, Format::Timecode, 25.0), "00:01:01:12");
        assert_eq!(Format::from_id("hh:mm:ss:ff"), Format::Timecode);
        assert_eq!(Format::from_id("nonsense"), Format::S);
    }

    #[test]
    fn parses_as_frame() {
        assert_eq!(parse_time("30", Format::S, 25.0), Some(30000.0));
        assert_eq!(parse_time("500ms", Format::S, 25.0), Some(500.0));
        assert_eq!(parse_time("2s", Format::Ms, 25.0), Some(2000.0));
        assert_eq!(parse_time("50 fr", Format::Ms, 25.0), Some(2000.0));
        assert_eq!(parse_time("1,500", Format::Ms, 25.0), Some(1500.0));
        assert_eq!(parse_time("01:01.25", Format::MinSec, 25.0), Some(61_250.0));
        assert_eq!(parse_time("1:2", Format::MinSec, 25.0), Some(62_000.0));
        assert_eq!(
            parse_time("01:01:01.005", Format::HourMinSec, 25.0),
            Some(3_661_005.0)
        );
        assert_eq!(
            parse_time("00:01:01:12", Format::Timecode, 25.0),
            Some(61_480.0)
        );
        assert_eq!(
            parse_time("00;01;01;12", Format::Timecode, 25.0),
            Some(61_480.0)
        );
        assert_eq!(parse_time("50", Format::Frames, 25.0), Some(2000.0));
        assert_eq!(parse_time("", Format::S, 25.0), None);
        assert_eq!(parse_time("abc", Format::S, 25.0), None);
        // Round trips.
        for f in FORMATS.iter().map(|f| f.0) {
            let shown = format_time(61_480.0, f, 25.0);
            assert_eq!(parse_time(&shown, f, 25.0), Some(61_480.0), "{f:?} {shown}");
        }
    }
}
