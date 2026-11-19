// SPDX-License-Identifier: AGPL-3.0-or-later
//! JavaScript number and string rules.
//!
//! FRAME was written in JavaScript, and its exports depend on JavaScript's
//! exact behavior: `Math.round` rounds halves up, `Number("…")` parses text
//! its own way and `String(1.5)` prints numbers in a specific format. The
//! exporters use these helpers so FrameCraft writes the same files.

/// `Math.round`: rounds to the nearest integer, halves toward +∞
/// (`-2.5` → `-2`, unlike Rust's `round`).
pub fn round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// `Math.max(lo, Math.min(hi, x))`.
pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    lo.max(hi.min(x))
}

/// `Number(x) || fallback`: missing, `NaN` and `0` all give the fallback.
pub fn or(x: Option<f64>, fallback: f64) -> f64 {
    match x {
        Some(v) if v != 0.0 && !v.is_nan() => v,
        _ => fallback,
    }
}

/// `Number.isFinite(x) ? x : fallback`.
pub fn finite_or(x: Option<f64>, fallback: f64) -> f64 {
    match x {
        Some(v) if v.is_finite() => v,
        _ => fallback,
    }
}

/// `x >>> 0`: ToUint32.
pub fn to_uint32(x: f64) -> f64 {
    if !x.is_finite() {
        return 0.0;
    }
    x.trunc().rem_euclid(4_294_967_296.0)
}

/// `Number(text)`: JavaScript's string to number conversion.
pub fn str_to_number(text: &str) -> f64 {
    let s = text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if s.is_empty() {
        return 0.0;
    }
    let (sign, body, signed) = match s.as_bytes()[0] {
        b'-' => (-1.0, &s[1..], true),
        b'+' => (1.0, &s[1..], true),
        _ => (1.0, s, false),
    };
    if body == "Infinity" {
        return sign * f64::INFINITY;
    }
    // Hex, octal and binary literals take no sign.
    if !signed && body.len() > 2 {
        let radix = match &body[..2] {
            "0x" | "0X" => 16,
            "0o" | "0O" => 8,
            "0b" | "0B" => 2,
            _ => 0,
        };
        if radix != 0 {
            return u128::from_str_radix(&body[2..], radix).map_or(f64::NAN, |v| v as f64);
        }
    }
    // Decimal: digits, one dot, optional exponent. Rust's parser accepts
    // words like "inf" and "nan", which JavaScript does not.
    let valid = !body.is_empty()
        && body
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
        && body
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '.');
    if !valid {
        return f64::NAN;
    }
    body.parse::<f64>().map_or(f64::NAN, |v| sign * v)
}

/// `String(x)`: JavaScript's number to text conversion (`Number#toString`).
pub fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return "0".into();
    }
    let sign = if x < 0.0 { "-" } else { "" };
    // Rust's `{:e}` gives the shortest digits that read back as the same
    // number, like JavaScript; only the layout differs.
    let sci = format!("{:e}", x.abs());
    let (mantissa, exp) = sci.split_once('e').expect("{:e} has an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp.parse::<i32>().expect("exponent is an integer") + 1;

    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let e = if e >= 0 {
            format!("+{e}")
        } else {
            e.to_string()
        };
        if k == 1 {
            format!("{digits}e{e}")
        } else {
            format!("{}.{}e{e}", &digits[..1], &digits[1..])
        }
    };
    format!("{sign}{body}")
}

/// `encodeURIComponent`.
pub fn encode_uri_component(s: &str) -> String {
    percent_encode(
        s,
        |b| b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b),
        false,
    )
}

/// `URLSearchParams` value encoding (`application/x-www-form-urlencoded`).
pub fn form_urlencode(s: &str) -> String {
    percent_encode(
        s,
        |b| b.is_ascii_alphanumeric() || b"*-._".contains(&b),
        true,
    )
}

fn percent_encode(s: &str, keep: impl Fn(u8) -> bool, space_plus: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if keep(b) {
            out.push(b as char);
        } else if b == b' ' && space_plus {
            out.push('+');
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `String.prototype.trim` (Unicode whitespace and the BOM).
pub fn trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_like_math_round() {
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(-2.6), -3.0);
        assert_eq!(round(0.49999999999999994), 0.0);
        assert_eq!(round(1.4), 1.0);
    }

    #[test]
    fn prints_numbers_like_javascript() {
        let cases = [
            (0.0, "0"),
            (-0.0, "0"),
            (5.0, "5"),
            (-5.0, "-5"),
            (1.5, "1.5"),
            (0.1 + 0.2, "0.30000000000000004"),
            (100.0, "100"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (123456789012345680000.0, "123456789012345680000"),
            (0.000001, "0.000001"),
            (0.0000001, "1e-7"),
            (1.5e-7, "1.5e-7"),
            (2.5e25, "2.5e+25"),
            (4294967295.0, "4294967295"),
            (f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
        ];
        for (x, want) in cases {
            assert_eq!(number_to_string(x), want, "String({x})");
        }
    }

    #[test]
    fn parses_strings_like_number() {
        assert_eq!(str_to_number(""), 0.0);
        assert_eq!(str_to_number("  42 "), 42.0);
        assert_eq!(str_to_number("-1.5e2"), -150.0);
        assert_eq!(str_to_number(".5"), 0.5);
        assert_eq!(str_to_number("0x1f"), 31.0);
        assert_eq!(str_to_number("Infinity"), f64::INFINITY);
        assert!(str_to_number("inf").is_nan());
        assert!(str_to_number("nan").is_nan());
        assert!(str_to_number("12px").is_nan());
        assert!(str_to_number("manual").is_nan());
        assert!(str_to_number("-0x10").is_nan());
    }

    #[test]
    fn uint32_and_or() {
        assert_eq!(to_uint32(-1.0), 4294967295.0);
        assert_eq!(to_uint32(4294967296.0 + 7.9), 7.0);
        assert_eq!(or(Some(0.0), 80.0), 80.0);
        assert_eq!(or(None, 80.0), 80.0);
        assert_eq!(or(Some(-3.0), 80.0), -3.0);
    }

    #[test]
    fn encodes_urls() {
        assert_eq!(encode_uri_component("a b/c?d=é"), "a%20b%2Fc%3Fd%3D%C3%A9");
        assert_eq!(encode_uri_component("it's(ok)!~*"), "it's(ok)!~*");
        assert_eq!(form_urlencode("a b&c=d~"), "a+b%26c%3Dd%7E");
    }
}
