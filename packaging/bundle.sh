#!/bin/sh
# Build murmur.app and install it to /Applications.
set -eu
cd "$(dirname "$0")/.."
cargo build --release
APP=target/murmur.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp packaging/Info.plist "$APP/Contents/Info.plist"
cp target/release/murmur "$APP/Contents/MacOS/murmur"
# Ad-hoc signature so macOS can attach Microphone/Accessibility grants to the app.
codesign --force --sign - "$APP"

rm -rf /Applications/murmur.app
cp -R "$APP" /Applications/
echo "Installed /Applications/murmur.app"
