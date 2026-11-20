// SPDX-License-Identifier: AGPL-3.0-or-later
//! Scales a captured PNG to the scene's size (macOS snapshots come in
//! device pixels: 2× on Retina). Area averaging, alpha kept.

/// `png` at `width` × `height`; unchanged when it already is.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn fit_png(png: Vec<u8>, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(&png));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (sw, sh) = (info.width as usize, info.height as usize);
    if (sw, sh) == (width as usize, height as usize) {
        return Ok(png);
    }
    // To RGBA.
    let src: Vec<[u8; 4]> = match info.color_type {
        png::ColorType::Rgba => buf
            .as_chunks::<4>()
            .0
            .iter()
            .take(sw * sh)
            .copied()
            .collect(),
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .take(sw * sh)
            .map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .take(sw * sh)
            .map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        _ => buf.iter().take(sw * sh).map(|&g| [g, g, g, 255]).collect(),
    };
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        let (y0, y1) = (y * sh / h, ((y + 1) * sh / h).max(y * sh / h + 1).min(sh));
        for x in 0..w {
            let (x0, x1) = (x * sw / w, ((x + 1) * sw / w).max(x * sw / w + 1).min(sw));
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let p = src[sy * sw + sx];
                    // Colour weighted by alpha, so transparent pixels do not darken edges.
                    let a = u32::from(p[3]);
                    acc[0] += u32::from(p[0]) * a;
                    acc[1] += u32::from(p[1]) * a;
                    acc[2] += u32::from(p[2]) * a;
                    acc[3] += a;
                    n += 1;
                }
            }
            let o = (y * w + x) * 4;
            for c in 0..3 {
                if let Some(v) = acc[c].checked_div(acc[3]) {
                    out[o + c] = v as u8;
                }
            }
            out[o + 3] = (acc[3] / n.max(1)) as u8;
        }
    }
    let mut png_out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().map_err(|e| e.to_string())?;
        wr.write_image_data(&out).map_err(|e| e.to_string())?;
    }
    Ok(png_out)
}

#[cfg(test)]
mod tests {
    use super::fit_png;

    fn encode(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(rgba).unwrap();
        out
    }

    #[test]
    fn halves_retina_snapshots() {
        // 2×2 → 1×1: red opaque, three transparent pixels.
        let src = encode(2, 2, &[255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let out = fit_png(src.clone(), 1, 1).unwrap();
        let mut dec = png::Decoder::new(std::io::Cursor::new(&out))
            .read_info()
            .unwrap();
        let mut buf = vec![0; dec.output_buffer_size().unwrap()];
        dec.next_frame(&mut buf).unwrap();
        // Colour stays red (not darkened by the transparent ones), alpha a quarter.
        assert_eq!(&buf[..4], &[255, 0, 0, 63]);
        // Same size: untouched.
        assert_eq!(fit_png(src.clone(), 2, 2).unwrap(), src);
    }
}
