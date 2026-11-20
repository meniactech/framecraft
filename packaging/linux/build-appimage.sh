#!/usr/bin/env bash
# Builds FrameCraft-<version>-<arch>.AppImage into target/dist/.
# Needs: cargo, curl, and the WebKitGTK 4.1 / GTK 3 libraries of the build
# system (they are bundled). Downloads linuxdeploy and its GTK plugin into
# target/appimage-tools/ on first run. Run from anywhere.
#
# Build on the oldest system the AppImage should run on (CI: Ubuntu 22.04):
# the bundled libraries need that glibc or newer.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
arch="$(uname -m)"
tools="$root/target/appimage-tools"
appdir="$root/target/appimage/FrameCraft.AppDir"
out="$root/target/dist/FrameCraft-$version-$arch.AppImage"

fetch() {
  if [ ! -s "$tools/$2" ]; then
    curl -fsSL -o "$tools/$2" "$1"
  fi
  chmod +x "$tools/$2"
}
mkdir -p "$tools" "$root/target/dist"
fetch "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$arch.AppImage" linuxdeploy
fetch "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh" linuxdeploy-plugin-gtk.sh
export PATH="$tools:$PATH"

cargo build --release -p framecraft-app

rm -rf "$root/target/appimage"
install -Dm755 target/release/framecraft "$appdir/usr/bin/framecraft"
mkdir -p "$appdir/usr/share/framecraft"
cp -r assets "$appdir/usr/share/framecraft/assets"
install -Dm644 LICENSE "$appdir/usr/share/doc/framecraft/copyright"
install -Dm644 THIRD_PARTY.md "$appdir/usr/share/doc/framecraft/THIRD_PARTY.md"

# WebKit starts helper processes (WebKitWebProcess, WebKitNetworkProcess)
# and loads its injected bundle from a directory compiled into the
# library. Copy that directory in, at the same path under usr/.
webkit_dir="$(dirname "$(find -L /usr/lib* -name WebKitWebProcess -type f 2>/dev/null | head -n1)")"
if [ -z "$webkit_dir" ] || [ "$webkit_dir" = "." ]; then
  echo "WebKitGTK helper processes not found (install libwebkit2gtk-4.1)" >&2
  exit 1
fi
mkdir -p "$appdir$(dirname "$webkit_dir")"
cp -r "$webkit_dir" "$appdir$webkit_dir"

# linuxdeploy: the app's libraries, the helpers' libraries, and GTK's
# runtime files (plugin). APPIMAGE_EXTRACT_AND_RUN: works without FUSE.
export APPIMAGE_EXTRACT_AND_RUN=1 DEPLOY_GTK_VERSION=3 NO_STRIP=1
linuxdeploy --appdir "$appdir" \
  --executable "$appdir/usr/bin/framecraft" \
  --deploy-deps-only "$appdir$webkit_dir" \
  --desktop-file packaging/linux/framecraft.desktop \
  --icon-file packaging/linux/framecraft.svg \
  --plugin gtk

# The bundled WebKit looks for its helpers under /usr: rewrite /usr as
# ././ (same length) so the paths are relative, and start the app from
# usr/ inside the AppImage. The AppImage runtime keeps the folder it was
# started from in $OWD (FrameCraft resolves relative paths against it).
find "$appdir" -name 'libwebkit2gtk-4.1.so*' -type f -exec sed -i -e 's|/usr|././|g' {} +
mkdir -p "$appdir/apprun-hooks"
cat > "$appdir/apprun-hooks/framecraft-webkit.sh" <<'HOOK'
# FrameCraft: the bundled WebKit finds its helper processes from here.
cd "$APPDIR/usr"
HOOK

rm -f "$out"
OUTPUT="$out" linuxdeploy --appdir "$appdir" --output appimage
echo "Built $out"
