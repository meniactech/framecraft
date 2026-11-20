#!/usr/bin/env bash
# Builds FrameCraft.app (one binary for Apple Silicon and Intel) and zips
# it as FrameCraft-<version>-macos.zip into target/dist/.
# Needs: macOS with Xcode command line tools, rustup. The app is not signed
# yet: the first start needs right-click → Open (or System Settings →
# Privacy & Security → Open Anyway).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
targets=(aarch64-apple-darwin x86_64-apple-darwin)

rustup target add "${targets[@]}"
for t in "${targets[@]}"; do
  cargo build --release -p framecraft-app --target "$t"
done

app="target/dist/FrameCraft.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/framecraft" \
  "target/aarch64-apple-darwin/release/framecraft" \
  "target/x86_64-apple-darwin/release/framecraft"
cp -R assets "$app/Contents/Resources/assets"
cp LICENSE THIRD_PARTY.md "$app/Contents/Resources/"

# App icon: an .icns from the rendered PNGs (packaging/icons/render.py).
iconset="target/dist/FrameCraft.iconset"
rm -rf "$iconset"
mkdir -p "$iconset"
for s in 16 32 128 256 512; do
  cp "packaging/icons/framecraft-$s.png" "$iconset/icon_${s}x${s}.png"
  cp "packaging/icons/framecraft-$((s * 2)).png" "$iconset/icon_${s}x${s}@2x.png"
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/FrameCraft.icns"
rm -rf "$iconset"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>FrameCraft</string>
  <key>CFBundleDisplayName</key><string>FrameCraft</string>
  <key>CFBundleIdentifier</key><string>tech.meniac.framecraft</string>
  <key>CFBundleExecutable</key><string>framecraft</string>
  <key>CFBundleIconFile</key><string>FrameCraft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
</dict>
</plist>
PLIST

zip="target/dist/FrameCraft-$version-macos.zip"
rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"
echo "Built $zip"
