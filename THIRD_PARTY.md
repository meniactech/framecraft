# Third-party material

Bundled assets live in [`assets/`](assets/) (see its README). MENiAC-made assets there are MIT ([assets/LICENSE](assets/LICENSE)).

Assets that FRAME bundled, checked before bringing them into FrameCraft. Only rows marked **OK** may be copied into this repository. Each bundled item keeps its own license; include the license text next to it when it is added.

| Asset (path in FRAME `static/`) | Source | License | Status |
|---|---|---|---|
| `fonts/Inter-VariableFont_opsz,wght.ttf` | [Inter](https://github.com/rsms/inter) | SIL OFL 1.1 | **Bundled** in `assets/fonts/` with its `OFL-*.txt` |
| `fonts/Montserrat-VariableFont_wght.ttf` | [Montserrat](https://github.com/JulietaUla/Montserrat) | SIL OFL 1.1 | **Bundled** in `assets/fonts/` with its `OFL-*.txt` |
| `fonts/MouseMemoirs-Regular.ttf` | Mouse Memoirs (Google Fonts) | SIL OFL 1.1 | **Bundled** in `assets/fonts/` with its `OFL-*.txt` |
| `fonts/OpenSans-VariableFont_wdth,wght.ttf` | [Open Sans](https://github.com/googlefonts/opensans) | SIL OFL 1.1 | **Bundled** in `assets/fonts/` with its `OFL-*.txt` |
| `fonts/Poppins-Regular.ttf` | [Poppins](https://github.com/itfoundry/Poppins) | SIL OFL 1.1 | **Bundled** in `assets/fonts/` with its `OFL-*.txt` |
| `fonts/Roboto-VariableFont_wdth,wght.ttf` | [Roboto](https://github.com/googlefonts/roboto-classic) | SIL OFL 1.1 | **Bundled** in `assets/fonts/` with its `OFL-*.txt` |
| `vendor/anime/`, `js/lib/anime.esm.min.js` | [anime.js](https://animejs.com) v4.3.6 | MIT | Bundled: `crates/framecraft-core/runtime/` |
| `vendor/chart/` | [Chart.js](https://www.chartjs.org) v4.5.1 | MIT | Bundled: `crates/framecraft-core/runtime/` |
| `vendor/qrcode/` | [node-qrcode](https://github.com/soldair/node-qrcode) v1.5.4 | MIT | Bundled: `crates/framecraft-core/runtime/` |
| `node_modules/html-to-image` (FRAME dependency) | [html-to-image](https://github.com/bubkoo/html-to-image) v1.11.13 | MIT | Bundled, preview only (Export → PNG Frame): `crates/framecraft-core/src/preview/html-to-image.js`, license next to it. Never in exports |
| `lottie/noto-emoji/` (878 animations, 65 MB) | [Noto Animated Emoji](https://googlefonts.github.io/noto-emoji-animation/) | CC BY 4.0 (to confirm) | Confirm license; consider downloading on demand instead of bundling 65 MB |
| `icons/align`, `icons/easing`, `icons/properties`, `icons/ui`, `icons/reel.svg` | Made for FRAME (Inkscape) | MENiAC | **Bundled** in `assets/icons/` under MIT (owner, 2026-11-19); `icons/spx` and `icons/ograf` excluded (trademarks) |
| `svg/basic/` (40 shapes), `samples/` | Made for FRAME (Inkscape) | MENiAC | **Bundled** in `assets/` under MIT (owner, 2026-11-19) |
| `particles/` (98 sprites) | Made for FRAME (owner) | MENiAC | **Bundled** in `assets/particles/` under MIT (owner, 2026-11-19) |
| `backgrounds/` (5 preview photos, 31 MB as PNG) | Pexels, Unsplash and Pixabay (owner, 2026-11-19) | Pexels / Unsplash / Pixabay licenses (free use incl. commercial; no selling or standalone redistribution) | **Bundled** as 1920 × 1080 JPEG in `assets/backgrounds/` (1.2 MB); editor-only, never exported |
| `palettes/stock-palettes.json`, `gradients/stock-gradients.json` | Made for FRAME | MENiAC | **Bundled** in `assets/` under MIT (owner, 2026-11-19) |
| `icons/spx/` | SPX Graphics logo | Trademark of SPX Graphics | Do not bundle without permission; use a text label |
| `icons/ograf/` | OGraf logo (EBU) | Trademark of EBU | Check EBU's logo terms |
| `images/frame_logo.png`, `images/ionframe_logo*.png` | MENiAC FRAME logos | MENiAC trademarks | Not for FrameCraft; it gets its own logo |
