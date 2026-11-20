#!/usr/bin/env bash
# Rebuild a Tauri macOS DMG with a pre-styled Finder layout.
#
# 背景：Tauri 的 bundle_dmg.sh 通过 Finder/AppleScript 设置图标位置，
# 在无 GUI、未授权自动化或 CI 环境中会失败，生成没有布局的普通 DMG。
# 本脚本改为复用仓库中的 .DS_Store 模板，不依赖 Finder：
#   1. 挂载原 DMG，取出 YeAPI.app；
#   2. 用应用、Applications 快捷方式、模板 .DS_Store 与卷图标重建 DMG。
#
# 用法：scripts/restyle-dmg.sh <path-to-dmg>
set -euo pipefail

DMG="${1:?usage: restyle-dmg.sh <dmg>}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TEMPLATE="$ROOT/src-tauri/bundle/dmg/DS_Store.template"
ICON="$ROOT/src-tauri/icons/icon.icns"

[[ -f "$DMG" ]] || { echo "DMG not found: $DMG" >&2; exit 1; }
[[ -f "$TEMPLATE" ]] || { echo "DMG layout template not found: $TEMPLATE" >&2; exit 1; }
[[ -f "$ICON" ]] || { echo "Volume icon not found: $ICON" >&2; exit 1; }

WORK="$(mktemp -d)"
MOUNT=""
cleanup() {
  if [[ -n "$MOUNT" ]]; then
    hdiutil detach "$MOUNT" >/dev/null 2>&1 || true
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

MOUNT="$(hdiutil attach -nobrowse -readonly "$DMG" | tail -1 | awk '{print $3}')"
[[ -n "$MOUNT" ]] || { echo "Failed to mount $DMG" >&2; exit 1; }

APP="$(find "$MOUNT" -maxdepth 1 -name '*.app' -print -quit)"
[[ -n "$APP" ]] || { echo "No .app found in $DMG" >&2; exit 1; }

cp -R "$APP" "$WORK/"
ln -s /Applications "$WORK/Applications"
cp -p "$TEMPLATE" "$WORK/.DS_Store"
cp -p "$ICON" "$WORK/.VolumeIcon.icns"
SetFile -c icnC "$WORK/.VolumeIcon.icns"
SetFile -a C "$WORK"
chmod 755 "$WORK"

hdiutil detach "$MOUNT" >/dev/null
MOUNT=""

NEW="${DMG%.dmg}-styled.dmg"
hdiutil create -volname YeAPI -srcfolder "$WORK" -fs HFS+ -format UDZO -ov "$NEW" >/dev/null
mv "$NEW" "$DMG"
echo "restyled $DMG"
