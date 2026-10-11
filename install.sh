#!/usr/bin/env bash
# release 构建 -> 打包 ePostit.app -> 安装到 /Applications 并启动
set -euo pipefail

APP="ePostit"
BIN="epostit"
ROOT="$(cd "$(dirname "$0")" && pwd)"
BUNDLE="$ROOT/target/$APP.app"
DEST="/Applications/$APP.app"

cd "$ROOT"
cargo build --release

rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources"
cp "packaging/Info.plist" "$BUNDLE/Contents/Info.plist"
cp "assets/AppIcon.icns" "$BUNDLE/Contents/Resources/AppIcon.icns"
cp "target/release/$BIN" "$BUNDLE/Contents/MacOS/$APP"

pkill -ix "$BIN" 2>/dev/null || true
rm -rf "$DEST"
cp -R "$BUNDLE" "$DEST"

# 刷新 LaunchServices 与图标缓存,保证 Finder/Spotlight 立即显示新图标
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
touch "$DEST"
"$LSREGISTER" -u "$DEST" 2>/dev/null || true
"$LSREGISTER" -f "$DEST"
killall iconservicesagent 2>/dev/null || true

open "$DEST"
