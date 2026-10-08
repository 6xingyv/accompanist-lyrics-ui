#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
version="${1:?Pass the demo version}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || exit 1
[[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]] || {
    echo 'The Flatpak packager requires Linux x86_64.' >&2; exit 1;
}
cd "$root"
branch=stable
if [[ "$version" == *-SNAPSHOT ]]; then branch=nightly
elif [[ "$version" == *-* ]]; then branch=beta; fi
work="$root/build/flatpak"
mkdir -p "$work" "$root/dist"
stage="$(mktemp -d "$work/package.XXXXXX")"
trap 'rm -rf -- "$stage"' EXIT
# Export the already-built Compose distribution; no SDK or sandbox build is needed.
python3 - "$root" "$stage/app" <<'PY'
import shutil, sys
from pathlib import Path
root, stage = map(Path, sys.argv[1:])
app = root / 'sample/desktop-app/build/compose/binaries/main/app/Accompanist'
if not (app / 'bin/Accompanist').is_file() or not (app / 'lib/runtime').is_dir():
    raise ValueError('Run :sample:desktop-app:createDistributable first')
assets = root / 'sample/desktop-app/packaging/flatpak'
app_id = 'com.mocharealm.accompanist.desktop'
files = stage / 'files'
shutil.copytree(app, files / 'lib/accompanist', symlinks=True)
launcher = files / 'bin/accompanist'
launcher.parent.mkdir(parents=True)
launcher.write_text('#!/bin/sh\nexec /app/lib/accompanist/bin/Accompanist "$@"\n')
launcher.chmod(0o755)
for source, destination in [
    (assets / f'{app_id}.desktop', f'share/applications/{app_id}.desktop'),
    (assets / f'{app_id}.metainfo.xml', f'share/metainfo/{app_id}.metainfo.xml'),
    (root / 'sample/android-app/src/main/ic_launcher-playstore.png', f'share/icons/hicolor/512x512/apps/{app_id}.png'),
]:
    target = files / destination
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)
(stage / 'metadata').write_text(
    f'[Application]\nname={app_id}\n'
    'runtime=org.freedesktop.Platform/x86_64/26.08\n'
    'sdk=org.freedesktop.Sdk/x86_64/26.08\n'
)
PY
flatpak build-finish --command=accompanist --socket=x11 --share=ipc --device=dri \
    --share=network --filesystem=home:ro '--talk-name=org.mpris.MediaPlayer2.*' \
    --talk-name=org.freedesktop.FileManager1 "$stage/app"
flatpak build-export "$work/repo" "$stage/app" "$branch"
flatpak build-bundle --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo \
    "$work/repo" "$stage/demo.flatpak" com.mocharealm.accompanist.desktop "$branch"
cp "$stage/demo.flatpak" "$root/dist/lyrics-ui-sample-$version-linux-x64.flatpak"
