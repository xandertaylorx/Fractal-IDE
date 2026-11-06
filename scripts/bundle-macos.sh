#!/usr/bin/env bash
# Build a double-clickable Fractal.app (macOS).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -p fractal-app
APP="target/Fractal.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/fractal "$APP/Contents/MacOS/fractal"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Fractal</string>
<key>CFBundleDisplayName</key><string>Fractal</string>
<key>CFBundleIdentifier</key><string>dev.fractal.ide</string>
<key>CFBundleExecutable</key><string>fractal</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>12.0</string>
</dict></plist>
PLIST
echo "Built $APP"
