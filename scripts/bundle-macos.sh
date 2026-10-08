#!/bin/sh
# Build Kave.app into target/release.
set -eu

cd "$(dirname "$0")/.."
cargo build --release

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
app=target/release/Kave.app

rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp target/release/kave "$app/Contents/MacOS/kave"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Kave</string>
    <key>CFBundleDisplayName</key>
    <string>Kave</string>
    <key>CFBundleIdentifier</key>
    <string>com.afifvdin.kave</string>
    <key>CFBundleExecutable</key>
    <string>kave</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>LSMinimumSystemVersion</key>
    <string>14.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

# An ad-hoc signature gives the app a stable identity, so the Input
# Monitoring permission survives until the next rebuild.
codesign --force --sign - "$app"

echo "Built $app"
