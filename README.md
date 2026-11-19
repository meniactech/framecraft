# FrameCraft

FrameCraft is a desktop editor for broadcast and web graphics: lower thirds, score bugs, brackets, tickers and banners. You design on a timeline with keyframes, much like a motion graphics app, and export packages that play in [SPX](https://spxgc.com), OGraf hosts and web browsers.

FrameCraft is the open-source successor of MENiAC FRAME. It is written in Rust and is in early development: nothing is usable yet.

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

Requires Rust 1.85 or newer ([rustup.rs](https://rustup.rs)).

```bash
cargo build
cargo test
cargo run -p framecraft-app
```

## License

FrameCraft is licensed under the GNU Affero General Public License v3.0 or later. See [LICENSE](LICENSE).

Graphics you export are yours. The playout code FrameCraft copies into exported packages is MIT-licensed, so exports carry no AGPL obligations. See [crates/framecraft-core/runtime/README.md](crates/framecraft-core/runtime/README.md).

Bundled fonts, animations and libraries keep their own licenses. See [THIRD_PARTY.md](THIRD_PARTY.md).
