# Built-in assets

Files FrameCraft uses at run time: icons, stock shapes, palettes and gradients, fonts and samples. The folder mirrors MENiAC FRAME's `static/`, so the URLs graphics use (`/svg/basic/shape-01.svg`, `/fonts/Inter-…ttf`, …) are the same.

| Path | What | License |
|---|---|---|
| `icons/` | Property tab, easing, alignment and UI icons | MIT, Oy MENiAC Ltd ([LICENSE](LICENSE)) |
| `svg/basic/` | 40 stock SVG shapes | MIT, Oy MENiAC Ltd |
| `palettes/stock-palettes.json` | Stock color palettes | MIT, Oy MENiAC Ltd |
| `gradients/stock-gradients.json` | Stock gradients | MIT, Oy MENiAC Ltd |
| `samples/lottie/` | Sample Lottie animation | MIT, Oy MENiAC Ltd |
| `images/transparent_checkers.png` | Transparency checkerboard | MIT, Oy MENiAC Ltd |
| `fonts/*.ttf` | Inter, Montserrat, Mouse Memoirs, Open Sans, Poppins, Roboto | SIL Open Font License 1.1 (`fonts/OFL-*.txt`) |

These were made for FRAME and are released under MIT so that anything that ends up in an exported graphic (shapes, palettes, gradients) carries no obligations.

Not here yet: particle sprites and preview plates (origin to be confirmed) and the Noto Animated Emoji (CC BY 4.0, 65 MB; to be downloaded or bundled with installers). Until then FrameCraft reads them from `FRAMECRAFT_BUILTIN_DIR` or a FRAME checkout. See [../THIRD_PARTY.md](../THIRD_PARTY.md).
