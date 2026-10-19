#!/usr/bin/env bash
set -euo pipefail

root="${1:?Pass the repository root}"
build_dir="${2:?Pass the desktop app build directory}"
app="$build_dir/compose/binaries/main/app/Accompanist.app"
icon_source="$root/sample/desktop-app/packaging/icons/Accompanist.icon"
resources="$app/Contents/Resources"
work="$build_dir/macos-icon"

[[ -d "$app" ]] || { echo "macOS app bundle not found: $app" >&2; exit 1; }
mkdir -p "$resources" "$work"
xcrun actool "$icon_source" --compile "$resources" \
    --platform macosx --minimum-deployment-target 12.0 \
    --target-device mac --app-icon Accompanist \
    --output-partial-info-plist "$work/icon-info.plist"
[[ -s "$resources/Assets.car" ]] || { echo 'Icon Composer did not produce Assets.car.' >&2; exit 1; }
python3 - "$app/Contents/Info.plist" "$work/icon-info.plist" <<'PY'
import plistlib
import sys
from pathlib import Path

info_path = Path(sys.argv[1])
partial_path = Path(sys.argv[2])
resources = info_path.parent / "Resources"
with info_path.open("rb") as source:
    info = plistlib.load(source)
with partial_path.open("rb") as source:
    icon_info = plistlib.load(source)

info.update(icon_info)
info["CFBundleIconName"] = "Accompanist"
if "CFBundleIconFile" not in info and (resources / "Accompanist.icns").is_file():
    info["CFBundleIconFile"] = "Accompanist.icns"
with info_path.open("wb") as output:
    plistlib.dump(info, output, fmt=plistlib.FMT_BINARY)
PY
