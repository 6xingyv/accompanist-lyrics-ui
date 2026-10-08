#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
version="${1:?Pass the demo version}"
build_number="${GITHUB_RUN_NUMBER:-1}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || exit 1
[[ "$(uname -s)" == Darwin ]] || { echo 'The iOS packager requires macOS and Xcode.' >&2; exit 1; }
cd "$root"
python3 tools/ios-local/package.py . --configuration release
work="$root/build/ios-package"
mkdir -p "$work" "$root/dist"
xcodegen generate --spec sample/ios-app/project.yml --project sample/ios-app
xcodebuild -project sample/ios-app/AccompanistIos.xcodeproj -scheme AccompanistIos \
    -configuration Release -sdk iphoneos -destination 'generic/platform=iOS' \
    -archivePath "$work/Accompanist.xcarchive" \
    CODE_SIGNING_ALLOWED=NO "MARKETING_VERSION=${version%%-*}" \
    "CURRENT_PROJECT_VERSION=$build_number" archive
app="$work/Accompanist.xcarchive/Products/Applications/Accompanist.app"
[[ -d "$app" ]] || { echo 'Xcode archive contains no demo app.' >&2; exit 1; }
# Use Apple's compiler for the existing Icon Composer source, then merge its icon keys.
xcrun actool sample/ios-app/Accompanist.icon --compile "$app" \
    --platform iphoneos --minimum-deployment-target 15.0 \
    --target-device iphone --target-device ipad --app-icon Accompanist \
    --output-partial-info-plist "$work/icon-info.plist"
python3 - "$app/Info.plist" "$work/icon-info.plist" "$version" <<'PY'
import plistlib, sys
from pathlib import Path
path = Path(sys.argv[1])
with path.open('rb') as source:
    info = plistlib.load(source)
with open(sys.argv[2], 'rb') as source:
    info.update(plistlib.load(source))
info['AccompanistDemoVersion'] = sys.argv[3]
if info.get('CADisableMinimumFrameDurationOnPhone') is not True:
    raise ValueError('Missing required Compose frame scheduling key')
with path.open('wb') as output:
    plistlib.dump(info, output, fmt=plistlib.FMT_BINARY)
PY
stage="$(mktemp -d "$work/payload.XXXXXX")"
trap 'rm -rf -- "$stage"' EXIT
mkdir "$stage/Payload"
cp -R "$app" "$stage/Payload/"
(cd "$stage" && zip -qry demo.ipa Payload)
cp "$stage/demo.ipa" "$root/dist/lyrics-ui-sample-$version-ios-arm64-unsigned.ipa"
