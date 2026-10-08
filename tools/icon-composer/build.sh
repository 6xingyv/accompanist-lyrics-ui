#!/usr/bin/env bash
# Build the two review patches without replacing the system xtool installation.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cache="${ACCOMPANIST_IOS_CACHE:-$HOME/.cache/accompanist-ios}/icon-composer"
export PATH="$HOME/.local/share/swiftly/bin:$PATH"
for executable in git rsync python3 swift make pkg-config; do
    command -v "$executable" >/dev/null || { echo "Missing dependency: $executable" >&2; exit 1; }
done
mkdir -p "$root/ref" "$cache"
cache="$(realpath "$cache")"
[[ "$cache" != "$root" && "$cache" != "$root/ref" ]] || exit 1

prepare_source() {
    local name="$1" revision="$2" patch="$3" source="$root/ref/$1" repository="${4:-$1}"
    if [[ ! -d "$source/.git" ]]; then
        git clone --depth 1 --no-checkout "https://github.com/xtool-org/$repository" "$source"
        if [[ "$(git -C "$source" rev-parse HEAD)" != "$revision" ]]; then
            git -C "$source" fetch --depth 1 origin "$revision"
        fi
        git -C "$source" checkout --detach "$revision"
    fi
    [[ "$(git -C "$source" rev-parse HEAD)" == "$revision" ]] || {
        echo "$name patch requires commit $revision; existing checkout is preserved." >&2; exit 1;
    }
    # A Windows checkout may contain CRLF even when its git blob uses LF.
    if git -c core.autocrlf=true -C "$source" apply --reverse --check "$patch" 2>/dev/null; then
        return
    fi
    git -c core.autocrlf=true -C "$source" apply --check "$patch"
    git -c core.autocrlf=true -C "$source" apply "$patch"
}
prepare_source AssetKit e763558b55fcbb5a443b1d7b2c6f0972d8bd14f7 "$root/tools/icon-composer/patches/assetkit.patch"
prepare_source xtool 491fe2fc2a39cedb56013a3c3dc2b1c1f2940bf7 "$root/tools/icon-composer/patches/xtool.patch"

sdk="${ACCOMPANIST_DARWIN_SDK:-$HOME/.swiftpm/swift-sdks/darwin.artifactbundle}"
runtime=xtool
if [[ -d "$sdk" ]] && { [[ ! -f "$sdk/darwin-sdk-version.txt" ]] || [[ "$(cat "$sdk/darwin-sdk-version.txt")" == "develop" ]]; }; then
    runtime=xtool-runtime
    prepare_source xtool-runtime 22655f3e70b87bef9be736e9625984e21bf8c697 \
        "$root/tools/icon-composer/patches/xtool-1.16.1.patch" xtool
fi

# Existing isolated host libraries can be used without modifying /usr. On other
# machines, install xtool's ordinary native development dependencies first.
if [[ -d "$cache/native/install/lib/pkgconfig" ]]; then
    export PKG_CONFIG_PATH="$cache/native/install/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
    export LD_LIBRARY_PATH="$cache/native/install/lib:${LD_LIBRARY_PATH:-}"
fi
for library in libplist-2.0 libusbmuxd-2.0 libimobiledevice-glue-1.0 libimobiledevice-1.0 openssl; do
    pkg-config --exists "$library" || { echo "Missing xtool development library: $library" >&2; exit 1; }
done
repositories=(AssetKit xtool)
if [[ "$runtime" == xtool-runtime ]]; then repositories+=(xtool-runtime); fi
for name in "${repositories[@]}"; do
    mkdir -p "$cache/$name"
    [[ "$(realpath "$cache/$name")" == "$cache/$name" && "$cache/$name" != "$root/ref/$name" ]] || exit 1
    rsync -a --delete --exclude=.git --exclude=.build --exclude=.tmp "$root/ref/$name/" "$cache/$name/"
done
for name in "${repositories[@]:1}"; do
python3 - "$cache/$name" <<'PY'
from pathlib import Path
import sys
root = Path(sys.argv[1])
manifest = root / 'Package.swift'
dependency = '.package(url: "https://github.com/xtool-org/AssetKit", .upToNextMinor(from: "1.0.0")),'
text = manifest.read_text()
if text.count(dependency) != 1:
    raise SystemExit('Unexpected AssetKit dependency declaration')
manifest.write_text(text.replace(dependency, '.package(path: "../AssetKit"),'))
# Windows checkouts use CRLF. SourceKit's Linux linter requires LF for correct
# token locations; normalize only this disposable build mirror.
for source in root.rglob('*.swift'):
    if '.build' not in source.parts:
        data = source.read_bytes()
        if b'\r\n' in data:
            source.write_bytes(data.replace(b'\r\n', b'\n'))
PY
done
cd "$cache/xtool"
swift build --product xtool --jobs "${ACCOMPANIST_SWIFT_JOBS:-4}"
export LINUX_SOURCEKIT_LIB_PATH="$(swift -print-target-info | python3 -c 'import json,sys,pathlib; print(pathlib.Path(json.load(sys.stdin)["paths"]["runtimeResourcePath"]).parent)')"
make lint
if [[ "$runtime" == xtool-runtime ]]; then
    echo 'Building Icon Composer backport for the installed legacy Darwin SDK (xtool 1.16.1).'
    cd "$cache/xtool-runtime"
    swift build --product xtool --jobs "${ACCOMPANIST_SWIFT_JOBS:-4}"
fi
binary="$(swift build --show-bin-path)/xtool"
mkdir -p "$cache/bin"
python3 - "$cache/bin/xtool" "$binary" "$cache/native/install/lib" "$(dirname "$cache")/icon-tools/usr/bin" <<'PY'
from pathlib import Path
import shlex, sys
p = Path(sys.argv[1])
library_path = shlex.quote(sys.argv[3])
svg_path = shlex.quote(sys.argv[4])
p.write_text('#!/usr/bin/env bash\nset -euo pipefail\n'
             f'export LD_LIBRARY_PATH={library_path}:"${{LD_LIBRARY_PATH:-}}"\n'
             f'export PATH={svg_path}:"$PATH"\n'
             f'exec {shlex.quote(sys.argv[2])} "$@"\n')
p.chmod(0o755)
PY
echo "Icon Composer xtool: $cache/bin/xtool"
