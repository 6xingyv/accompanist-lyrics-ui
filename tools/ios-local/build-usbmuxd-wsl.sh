#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source="$root/ref/usbmuxd"
if [[ ! -d "$source/.git" ]]; then
    git clone --depth 1 --branch 1.1.1 https://github.com/libimobiledevice/usbmuxd "$source"
fi
[[ "$(git -C "$source" rev-parse HEAD)" == "$(git -C "$source" rev-parse '1.1.1^{commit}')" ]] || {
    echo 'The USB/IP adapter requires the usbmuxd 1.1.1 source tag.' >&2; exit 1;
}
work="${ACCOMPANIST_IOS_CACHE:-$HOME/.cache/accompanist-ios}/usbmuxd-wsl"
mkdir -p "$work"
git -C "$source" archive HEAD | tar -xf - -C "$work"
cd "$work"
python3 - <<'PY'
from pathlib import Path
p=Path('src/usb.h')
s=p.read_text()
# End every maximum-sized transfer with a short USB packet. USB/IP can delay
# a separately submitted ZLP, allowing adjacent writes to merge on the device.
s=s.replace('#define USB_MTU (3 * 16384)', '#define USB_MTU (16384 - 1)')
p.write_text(s)
# libplist 2.3 added its own plist_format_t enum. Keep the 1.1.1 utility enum
# private so this isolated build retains its original 0/1 format values.
for p in Path('src').iterdir():
    if p.suffix in ('.c', '.h'):
        s=p.read_text().replace('plist_format_t', 'usbmuxd_plist_format_t')
        s=s.replace('PLIST_FORMAT_XML', 'USBMUXD_PLIST_FORMAT_XML').replace('PLIST_FORMAT_BINARY', 'USBMUXD_PLIST_FORMAT_BINARY')
        p.write_text(s)
PY
./autogen.sh --without-preflight --without-systemd --prefix="$work/install" > build.log 2>&1
make -j2 >> build.log 2>&1
echo "Built $work/src/usbmuxd"
