# Playout runtime

Files copied into every exported package. They run inside SPX, OGraf hosts, browsers and MENiAC Air, not inside FrameCraft.

## License of exported graphics

FrameCraft itself is AGPL-3.0-or-later. **The code FrameCraft puts into exported packages is MIT-licensed** ([LICENSE](LICENSE)), so graphics made with FrameCraft can be used, sold and modified without any AGPL obligations. That covers:

- `runtime/ionframe-spx-runtime.mjs`
- everything in `src/export/html/`: the SPX bridge (`spx_interface.js`), the page skeletons and the chart, QR, Lottie, video, BOARD and start-up script blocks
- `src/export/ograf_component.mjs.in`, the OGraf Web Component
- the markup and data the exporters write around them

The files carry no license header because the exporter tests compare them byte for byte with FRAME's output; this README and [LICENSE](LICENSE) are the license statement.

| File | Package path | What | License |
|---|---|---|---|
| `ionframe-spx-runtime.mjs` | `js/ionframe-spx-runtime.mjs` | Timeline playback, seeking (`goToTime`), modifiers, live data. Written for MENiAC FRAME. | MIT ([LICENSE](LICENSE)) |
| `lib/anime.esm.min.js` | `js/lib/anime.esm.min.js` | [Anime.js](https://animejs.com) v4.3.6 | MIT, see below |
| `lib/chart.umd.min.js` | `js/lib/chart.umd.min.js` | [Chart.js](https://www.chartjs.org) v4.5.1 | MIT, see below |
| `lib/qrcode.min.js` | `js/lib/qrcode.min.js` | [node-qrcode](https://github.com/soldair/node-qrcode) v1.5.4, browser build | MIT, see below |

Keep these byte-identical to the versions FRAME shipped until FrameCraft's own reference tests are updated: the exporter tests compare package contents by SHA-256.

## MIT license notices

Anime.js: Copyright (c) 2026 Julian Garnier

Chart.js: Copyright (c) 2014-2025 Chart.js Contributors

node-qrcode: Copyright (c) 2012 Ryan Day

Each of the above is licensed under the MIT License:

> Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
