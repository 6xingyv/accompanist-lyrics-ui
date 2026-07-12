#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/../.." && pwd)"
cargo_root="$repo_root/text_engine"
build_root="$repo_root/build/appimage"
appdir="$build_root/AppDir"
dist_dir="$repo_root/dist"
tools_dir="$build_root/tools"

if [[ "$(uname -s)" != "Linux" ]]; then
    echo "AppImage packaging must run on Linux" >&2
    exit 2
fi

case "$(uname -m)" in
    x86_64) appimage_arch="x86_64" ;;
    aarch64|arm64) appimage_arch="aarch64" ;;
    *) echo "unsupported AppImage architecture: $(uname -m)" >&2; exit 2 ;;
esac

if [[ -n "${VERSION:-}" ]]; then
    version="$VERSION"
else
    version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' \
        "$cargo_root/crates/lyrics-desktop/Cargo.toml" | head -n 1)"
fi
output="${OUTPUT:-$dist_dir/Accompanist-Lyrics-${version}-${appimage_arch}.AppImage}"
target_dir="${CARGO_TARGET_DIR:-$cargo_root/target}"
binary="$target_dir/release/desktop_lyrics"
linuxdeploy="${LINUXDEPLOY:-$tools_dir/linuxdeploy-${appimage_arch}.AppImage}"
icon="$build_root/accompanist-lyrics.png"

mkdir -p -- "$build_root" "$dist_dir" "$tools_dir"
rm -rf -- "$appdir"
mkdir -p -- "$appdir/usr/bin"

echo "[appimage] building lyrics-desktop $version"
(cd -- "$cargo_root" && cargo build --locked --release -p lyrics-desktop --bin desktop_lyrics)
install -Dm755 -- "$binary" "$appdir/usr/bin/desktop_lyrics"
cp -- "$repo_root/sample/android-app/src/main/ic_launcher-playstore.png" "$icon"

if [[ ! -x "$linuxdeploy" ]]; then
    if [[ -n "${LINUXDEPLOY:-}" ]]; then
        echo "LINUXDEPLOY is not executable: $linuxdeploy" >&2
        exit 3
    fi
    echo "[appimage] downloading linuxdeploy for $appimage_arch"
    curl --fail --location --retry 3 \
        "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-${appimage_arch}.AppImage" \
        --output "$linuxdeploy"
    chmod +x -- "$linuxdeploy"
fi

chmod +x -- "$script_dir/AppRun"
export ARCH="$appimage_arch"
export OUTPUT="$output"

echo "[appimage] collecting runtime libraries"
"$linuxdeploy" --appimage-extract-and-run \
    --appdir "$appdir" \
    --executable "$appdir/usr/bin/desktop_lyrics" \
    --desktop-file "$script_dir/accompanist-lyrics.desktop" \
    --icon-file "$icon" \
    --custom-apprun="$script_dir/AppRun" \
    --output appimage

chmod +x -- "$output"
echo "[appimage] wrote $output"
