# FrameCraft

FrameCraft is a desktop editor for broadcast and web graphics: lower thirds, score bugs, brackets, tickers and banners. You design on a timeline with keyframes, much like a motion graphics app, and export packages that play in [SPX](https://spxgc.com), OGraf hosts and web browsers.

FrameCraft is the open-source successor of [MENiAC FRAME](https://meniac.tech). It is written in Rust (the UI too, with [Dioxus](https://dioxuslabs.com)) and is under active development: the editor works for daily use on Linux, installers come next.

## What it does

- **Editor:** viewport with direct manipulation (move, resize, rotate, pivot, polygon points), outliner, properties, timeline with keyframes, easing, cues and auto-key, undo for everything.
- **Layers:** text, shapes, polygons and paths, images and SVG shapes, image slots, charts, QR codes, particles, lightning, patterns, Lottie and video, with modifiers (masks, blur, color, shadow, skew, crop, text effects, path follow).
- **Libraries:** project media, motions (house and your own), components, palettes and gradients.
- **Scripts and live data:** per-graphic scripts called from cues; [BOARD](https://board.meniac.tech) live data.
- **Projects:** a project is a folder (`Name.framecraft/`) with autosave and recovery. FRAME project and graphic packs open and export both ways.

## What it exports

- **SPX templates**: HTML templates for SPX Graphics Controller.
- **OGraf packages**: the EBU OGraf format, as a Web Component.
- **HTML banners**: self-contained animated HTML.

Exported graphics are plain HTML, CSS and JavaScript. Every package also carries `meta/air-render.json`, which lets MENiAC Air render it to video.

## Layout

| Crate | What |
|---|---|
| `crates/framecraft-core` | Document model, timeline evaluation, motions, modifiers and exporters. No UI. |
| `crates/framecraft-app` | The desktop app (`framecraft` binary). |

## Build

Requires Rust 1.88 or newer ([rustup.rs](https://rustup.rs)). On Linux the desktop app needs WebKitGTK:

```bash
# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev
```

```bash
cargo build
cargo test
cargo run -p framecraft-app -- path/to/project.framecraft   # or a .frameproject.zip
```

`FRAMECRAFT_DEBUG=1` logs preview, bridge and autosave activity.

### Built-in assets

Built-in fonts, stock shapes, palettes, gradients, particle sprites, preview plates, icons and the Noto emoji animations are not in this repository yet; their licenses are being sorted out (see [THIRD_PARTY.md](THIRD_PARTY.md)). Until they ship with FrameCraft, the app reads them from a folder set in `FRAMECRAFT_BUILTIN_DIR` (or a FRAME checkout next to this one). Without them FrameCraft still works, but exports use system fonts and the stock libraries are empty.

### Reference tests

Tests that compare exports byte for byte with FRAME's run when FRAME's reference set is available (`FRAMECRAFT_REFERENCE`, or a FRAME checkout next to this one) and skip otherwise.

## License

FrameCraft is licensed under the GNU Affero General Public License v3.0 or later. See [LICENSE](LICENSE).

Graphics you export are yours. The playout code FrameCraft copies into exported packages is MIT-licensed, so exports carry no AGPL obligations. See [crates/framecraft-core/runtime/README.md](crates/framecraft-core/runtime/README.md).

Bundled fonts, animations and libraries keep their own licenses. See [THIRD_PARTY.md](THIRD_PARTY.md).
