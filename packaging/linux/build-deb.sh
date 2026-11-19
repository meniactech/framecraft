#!/usr/bin/env bash
# Builds framecraft_<version>_<arch>.deb into target/dist/.
# Needs: cargo, dpkg-deb. Run from anywhere.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
arch="$(dpkg --print-architecture)"
pkg="framecraft_${version}_${arch}"
stage="target/dist/$pkg"

cargo build --release -p framecraft-app

rm -rf "$stage"
install -Dm755 target/release/framecraft "$stage/usr/bin/framecraft"
mkdir -p "$stage/usr/share/framecraft"
cp -r assets "$stage/usr/share/framecraft/assets"
install -Dm644 packaging/linux/framecraft.desktop "$stage/usr/share/applications/framecraft.desktop"
install -Dm644 packaging/linux/framecraft.svg "$stage/usr/share/icons/hicolor/scalable/apps/framecraft.svg"
install -Dm644 LICENSE "$stage/usr/share/doc/framecraft/copyright"
install -Dm644 THIRD_PARTY.md "$stage/usr/share/doc/framecraft/THIRD_PARTY.md"

size="$(du -sk "$stage" | cut -f1)"
mkdir -p "$stage/DEBIAN"
cat > "$stage/DEBIAN/control" <<CONTROL
Package: framecraft
Version: $version
Architecture: $arch
Maintainer: MENiAC <72707698+meniactech@users.noreply.github.com>
Installed-Size: $size
Depends: libwebkit2gtk-4.1-0, libgtk-3-0, libxdo3
Section: graphics
Priority: optional
Homepage: https://github.com/meniactech/framecraft
Description: Editor for animated broadcast and web graphics
 FrameCraft designs lower thirds, score bugs, tickers and banners on a
 keyframe timeline and exports SPX templates, OGraf packages and HTML
 banners. Open-source successor of MENiAC FRAME.
CONTROL

dpkg-deb --build --root-owner-group "$stage" "target/dist/$pkg.deb"
echo "Built target/dist/$pkg.deb"
