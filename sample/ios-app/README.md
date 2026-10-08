# iOS sample

The SwiftUI shell hosts the Compose player presentation from `sample/shared`,
reusing Android/desktop background, phone/wide layout, artwork, header, caption
controls and lyrics typography. Metadata marquee also matches the
`feature-text-engine` renderer: playback-clock phase, 1600ms hold, 40px/s period,
smoothstep travel, a full viewport plus 48px between copies and fades confined
to neighbouring padding. Title/artist use 17sp/600 and 15sp/400 with 1.3× line height.
The entire page uses the album artwork background; audio tags supply title,
artist and cover. iOS uses its default system font and packages no bundled fonts.
The More button directly opens the same shared Open local files dialog as Android.
Select audio, optional lyrics and optional timed translation inside it, then press
Play to import and start playback. File selection and Cancel preserve the current
track. Import errors stay in the dialog for correction. Click lyrics to seek, and
switch translation or pronunciation. Audio uses
AVAudioPlayer; CADisplayLink samples its position for the shared lyrics clock.

## Local Windows / Linux build

No remote Mac is used by this build. On Windows, install WSL2 and run the following
inside its Linux distribution:

- JDK 17+, Python 3, rsync and curl.
- Swift through swiftly, xtool, and its Darwin Swift SDK.
- An Android SDK accessible to Gradle, and the project's local Maven dependencies.

The existing machine uses Swift 6.3.1, xtool 1.16.1 and the iPhoneOS 26.4 SDK.
The sample's `Accompanist.icon` uses experimental Icon Composer support from
the local AssetKit/xtool patches. Build the patched tool first with
`bash tools/icon-composer/build.sh` from the repository root. See
`tools/icon-composer/README.md` for prerequisites and contribution status.
The iOS adapter uses its cache wrapper when available, or an explicit
`ACCOMPANIST_XTOOL_BIN`; stock xtool 1.16.1 cannot compile this `.icon` input.
Install the Swift SDK using xtool's Linux installation instructions. The script
finds it at `~/.swiftpm/swift-sdks/darwin.artifactbundle`; override this with
`ACCOMPANIST_DARWIN_SDK` if needed.

From the repository root on Windows:

```powershell
.\tools\ios-local\build.ps1
```

From Linux / WSL:

```bash
bash tools/ios-local/build.sh
```

The script uses an isolated Linux source mirror, downloads Kotlin/Native 2.4.10
and the matching iOS runtime, compiles the static Kotlin framework, wraps it as an
ARM64 XCFramework, copies Compose resources, and invokes `xtool dev build --ipa`.
It preserves the installed Kotlin compiler and Windows project build caches.
When the local `lyrics-core` iOS publication is missing, it compiles the sibling
`../lyrics-core` source in a separate mirror and publishes only its iOS artifact.

Outputs:

- `build/ios-local/Accompanist.ipa`
- `build/ios-local/build.log`
- `sample/ios-app/Frameworks/AccompanistSample.xcframework`

The cache defaults to `~/.cache/accompanist-ios`. Optional environment variables:
`ACCOMPANIST_IOS_CACHE`, `ACCOMPANIST_MAVEN_REPOSITORY` (default `/mnt/e/maven`),
`ACCOMPANIST_CORE_SOURCE`, and `ANDROID_HOME`. Under WSL the Android SDK is also
read from the root `local.properties` and translated to its Linux mount path.

## Adapter scope

Kotlin/Native's official Apple targets require macOS. This local build adapts
the host checks and SDK/toolchain paths with a process-local Java agent, pinned to
Kotlin 2.4.10. It uses the matching Apple target runtime and xtool's Linux Mach-O
tools; it does not change upstream compiler JARs. Apple's debug stepping extension
is disabled because Linux LLVM does not implement it. Revisit the adapter when
upgrading Kotlin or the Darwin SDK.

The output is a device ARM64 build. Installing it on an iPhone additionally needs
xtool signing/provisioning and a connected device; producing the IPA alone does
not verify it on hardware. Simulator and App Store distribution are outside this
sample build.

Keep `CADisableMinimumFrameDurationOnPhone=true` in `Info.plist`. Compose checks
this at startup and aborts if it is missing or false. `package.py` validates the
key before preparing the app; it also enables ProMotion frame scheduling.

On macOS, the same Kotlin sources use the ordinary Apple toolchain. Compile
`:sample:shared:linkDebugFrameworkIosArm64`, prepare the XCFramework/resources
with `python3 tools/ios-local/package.py .`, then run `xtool dev build --ipa`
from this directory.

## Install on a connected iPhone

With xtool already authenticated, unlock the phone, accept its trust prompt, and run:

```bash
xtool install --usb --udid <device-udid> /path/to/Accompanist.ipa
```

xtool provisions and signs the unsigned build before installing it. The sample
was successfully installed locally on an iPhone running iOS 26.7. Its signed
identifier uses xtool's team prefix, followed by
`com.mocharealm.accompanist.ios.sample`. Opening it from the phone is supported;
the installed xtool 1.16.1 reported a debugserver error when asked to launch it.

### WSL USB/IP transfer workaround

On this machine, transferring through usbipd-win with the stock usbmuxd 1.1.1
stalled with `asyncReadComplete, message was too large (65536 bytes, max = 65535)`.
The isolated build in `tools/ios-local/build-usbmuxd-wsl.sh` uses a 16383-byte
maximum transfer, ensuring maximum-sized transfers end with a short USB packet
instead of relying on a separate zero-length transfer. Installation completed
after using that daemon. This is a local workaround; the system binary is preserved.

To rebuild it inside WSL, install `libusb-1.0-0-dev`, `libplist-dev`, `libtool-bin`,
`pkg-config`, `autoconf`, `automake`, a C compiler and make, then run:

```bash
bash tools/ios-local/build-usbmuxd-wsl.sh
```

For installation, share only the iPhone with `usbipd bind --busid <bus> --force`,
reconnect it if Windows still holds the old connection, and attach it using
`usbipd attach --wsl --busid <bus>`. Temporarily run the built daemon in place of
the stock daemon, retaining its `usbmux` user and `/var/lib/lockdown` pairing
records. Detach/unbind the phone and restore the stock service after installation.
