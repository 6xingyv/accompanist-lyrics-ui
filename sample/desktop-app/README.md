# Desktop lyrics

The desktop sample follows an external player. Android, desktop and iOS reuse the
player presentation in `sample/shared`: background, phone/wide layout, artwork,
header, caption controls and lyrics typography. Desktop keeps its own title bar.
The header menu contains media-session scanning, lyrics scanning, configuration
reload and configuration-file location.
Desktop lyrics use the `feature-text-engine` resize rule: landscape content at
least 600dp wide and 1.3:1 grows from 1.0× to 1.4× over 1024–1600dp width and
512–1000dp height, using whichever dimension limits growth. Main, accompaniment,
translation and pronunciation text scale together; portrait text keeps its base size.
Metadata uses the same feature-text-engine marquee: playback-clock phase, 1600ms
hold, 40px/s period with smoothstep travel, a full viewport plus 48 renderer pixels
between copies, and fading only in the 8dp padding beside the text column. The
normal column stays solid. Title/artist use 17sp/600 and 15sp/400, 1.3× line height,
white/40% white and Plus blending. Non-overflowing text bypasses the fade mask.

## Start

```powershell
.\gradlew.bat :sample:desktop-app:run --args="--lyrics-dir C:\Users\Simon\Music\Local --recursive"
```

On macOS or Linux, use `./gradlew` and your own lyrics directory. Gradle selects
JetBrains Runtime 21 for compilation, launch and packaging, downloading it through
the toolchain resolver if needed. The bundled JVM, media adapter and Compose native
renderer follow the host architecture, including Apple Silicon.

Windows/macOS native media compilation resolves a separate Azul OpenJDK 21 SDK
for JNI headers. Android Studio's bundled JBR has `javac` but omits those headers;
using it as the Gradle JVM is supported. This SDK only supplies build headers;
the application and its window integration still run on JBR.

On Windows and macOS, the window uses JBR's `WindowDecorations.CustomTitleBar`.
JBR owns caption dragging, double-click, native window buttons and border resizing.
Hovering the caption reveals the native buttons and an SF Symbols `pin.fill` /
`pin.slash` toggle with a tooltip. Windows gives it the same rectangular cell size
as minimize/maximize/close, directly beside them. macOS uses a small rounded button
beside the left traffic lights. The rendered button and native client hit region
share the same bounds. Linux keeps the window manager's normal native decorations.
Rescanning media sessions reconnects the platform adapter and returns to media
following. Rescanning lyrics refreshes the directory index and reparses the current
track (or the active local preview). The configuration-location action reveals
`config.json` in the system file manager, falling back to opening its folder.
Clicking a lyric requests a seek in the followed player. Space controls playback;
a local preview advances only its own timeline. The header's translation and
pronunciation buttons use the shared Android/iOS appearance. Display choices
persist across launches.

Shortcuts use Command on macOS and Ctrl elsewhere: O opens a preview, R reloads
lyrics, P toggles pinning, W closes the window, and Left/Right seeks by five seconds.
Space toggles playback.

## Media adapters

| Platform | Source | Requirements |
| --- | --- | --- |
| Windows | Current SMTC session through C++/WinRT + JNI | Windows 10 1809+; a player publishing media metadata/timeline. |
| Linux | MPRIS2 over the session D-Bus | An MPRIS player and a desktop session bus. The app includes its Java Unix socket transport; `playerctl` is not required. |
| macOS | Music and Spotify through native ScriptingBridge + JNI | macOS 12+; allow Accompanist to control the player in System Settings → Privacy & Security → Automation. Other macOS players are not yet connected. |

Media calls run in-process on a dedicated native worker. Windows COM initialization
and release stay on that worker. There are no PowerShell/osascript child processes
or JSON media messages. Native position reads record `System.nanoTime()` immediately,
before artwork work, so a slow cover read does not become a clock offset.

Playback chooses a playing MPRIS/macOS source first. Metadata, playback state and
position are polled approximately every 500 ms, with interpolation between samples.
Windows Apple Music uses distinct timeline publications and an eight-sample clock
average. Rapid lyric clicks supersede queued older seeks; late pre-seek samples are
held until acknowledgment or timeout. Seeking depends on the player's capabilities.

Cover images are decoded at a bounded resolution and passed to the shared Android
background component. There is no audio loopback capture or audio-reactive mesh.

## Lyrics matching

The directory index accepts TTML/XML, LRC/ELRC, LYS, YRC and KRC. It matches title,
`artist - title` or `title - artist`, ignoring case and punctuation and allowing
track-number prefixes and fuzzy spelling differences. Subdirectories are included
only with `recursive: true` or `--recursive`. Text and compressed `krc1` files are
supported. Lyrics are parsed outside the UI thread.

## Configuration

On first launch the app creates `config.json` and `lyrics/` under:

- Windows: `%LOCALAPPDATA%\Accompanist`
- macOS: `~/Library/Application Support/Accompanist`
- Linux: `$XDG_CONFIG_HOME/accompanist-lyrics` or `~/.config/accompanist-lyrics`

```json
{
  "lyrics_dir": "lyrics",
  "recursive": false,
  "gpu": "system",
  "frame_timing": false,
  "window": {
    "width": 420,
    "height": 520,
    "min_width": 320,
    "min_height": 240,
    "always_on_top": true
  }
}
```

Relative lyrics paths in the configuration are relative to that file. Command-line
and environment paths are relative to the current working directory. Precedence is
command line, environment, configuration. Reload configuration applies the lyrics directory, recursive scanning
and window size, minimum size and pinning without restarting. Invalid configuration
keeps the previous working settings. GPU and renderer tracing settings require a
restart; matching/parser timing follows the reloaded configuration.

Flags: `--config <file>`, `--lyrics-dir <folder>`, `--recursive`,
`--gpu system|high_performance|minimum_power`, `--frame-timing`, `--help`.

Environment overrides: `ACCOMPANIST_LYRICS_DIR`, `ACCOMPANIST_LYRICS_RECURSIVE`,
`ACCOMPANIST_GPU`, `ACCOMPANIST_FRAME_TIMING`.

GPU preferences map to Skiko's auto/discrete/integrated selection before the first
window. They are supported by the Direct3D/Metal backends; Linux OpenGL adapter
selection remains controlled by the graphics driver/environment.

With frame timing enabled, stderr contains Skiko FPS/long-frame reports, directory
matching/parser time, and library CPU timings for layout, row drawing, phonetic
drawing, rasterization and cache hits. These are host CPU durations; they are not
separate GPU execution/flush measurements.

## Packages

Build on the target OS. Windows requires Visual Studio C++ tools (x64, or ARM64
when building on Windows ARM64) and the Windows SDK with C++/WinRT headers. macOS
requires Xcode Command Line Tools. `processResources` builds the platform JNI
library and embeds it in the app; end users need no native compiler.

```text
./gradlew :sample:desktop-app:packageDmg
.\gradlew.bat :sample:desktop-app:packageExe
```

`createDistributable` creates the application directory with its bundled JBR.
The macOS bundle has a stable bundle identifier, an Apple Events usage description
and hardened-runtime entitlements for automation, JVM JIT and native libraries.
Release distribution still needs the developer's signing/notarization
credentials. There is no AppImage packaging.
