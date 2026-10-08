#!/usr/bin/env bash
# Kotlin 2.4.10 + xtool: local iOS ARM64 build on Linux/WSL, without a remote Mac.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cache="${ACCOMPANIST_IOS_CACHE:-$HOME/.cache/accompanist-ios}"
xtool_bin="${ACCOMPANIST_XTOOL_BIN:-xtool}"
if [[ -z "${ACCOMPANIST_XTOOL_BIN:-}" && -x "$cache/icon-composer/bin/xtool" ]]; then
    xtool_bin="$cache/icon-composer/bin/xtool"
fi
sdk="${ACCOMPANIST_DARWIN_SDK:-$HOME/.swiftpm/swift-sdks/darwin.artifactbundle}"
version=2.4.10
project_version="$(sed -n 's/^kotlin = "\([^"]*\)"/\1/p' "$root/gradle/libs.versions.toml" | tr -d '\r')"
[[ "$project_version" == "$version" ]] || { echo "The local adapter supports Kotlin $version; project uses $project_version." >&2; exit 1; }
export PATH="$HOME/.local/share/swiftly/bin:$PATH"
[[ "$(uname -sm)" == "Linux x86_64" ]] || { echo 'This adapter requires Linux x86_64 (including WSL2).' >&2; exit 1; }
for executable in java javac python3 rsync curl swift "$xtool_bin"; do
    command -v "$executable" >/dev/null || { echo "Missing dependency: $executable" >&2; exit 1; }
done
[[ -d "$sdk" ]] || { echo 'Install the Darwin Swift SDK with xtool first.' >&2; exit 1; }
mkdir -p "$cache" "$root/build/ios-local"
cache="$(realpath "$cache")"
exec > >(tee "$root/build/ios-local/build.log") 2>&1

native="$HOME/.konan/kotlin-native-prebuilt-linux-x86_64-$version"
if [[ ! -f "$native/konan/lib/kotlin-native-compiler-embeddable.jar" ]]; then
    curl -fL --retry 3 "https://github.com/JetBrains/kotlin/releases/download/v$version/kotlin-native-prebuilt-linux-x86_64-$version.tar.gz" -o "$cache/native-linux.tar.gz"
    mkdir -p "$HOME/.konan"
    tar -xzf "$cache/native-linux.tar.gz" -C "$HOME/.konan"
fi
runtime="$cache/mac-runtime/kotlin-native-prebuilt-macos-aarch64-$version/konan/targets/ios_arm64"
if [[ ! -f "$runtime/native/compiler_interface.bc" ]]; then
    mkdir -p "$cache/mac-runtime"
    # Reuse a downloaded archive from the initial local build, when present.
    archive="$root/build/ios-local/kotlin-native-macos-$version.tar.gz"
    if [[ ! -f "$archive" ]]; then
        archive="$cache/native-macos.tar.gz"
        curl -fL --retry 3 "https://github.com/JetBrains/kotlin/releases/download/v$version/kotlin-native-prebuilt-macos-aarch64-$version.tar.gz" -o "$archive"
    fi
    tar -xzf "$archive" -C "$cache/mac-runtime" --wildcards '*/konan/targets/ios_arm64/*'
fi
overlay="$cache/kotlin-native-$version"
if [[ ! -d "$overlay" ]]; then
    mkdir -p "$overlay"
    cp -as "$native/." "$overlay/"
fi
ln -sfn "$runtime" "$overlay/konan/targets/ios_arm64"
# Replace only the overlay's symlink: the installed compiler remains untouched.
if [[ -L "$overlay/konan/konan.properties" ]]; then unlink "$overlay/konan/konan.properties"; fi
cp "$native/konan/konan.properties" "$overlay/konan/konan.properties"
cat >> "$overlay/konan/konan.properties" <<'PROPERTIES'

targetToolchain.linux_x64-ios_arm64=unused
additionalToolsDir.linux_x64=unused
llvmHome.linux_x64=llvm-21-x86_64-linux-essentials-116
PROPERTIES

toolchain="$cache/toolchain"
mkdir -p "$toolchain/bin" "$toolchain/lib" "$cache/agent-classes"
# LLD chooses its linker dialect from argv[0]; a symlink named ld selects ELF.
printf '#!/usr/bin/env bash\nexec "%s" "$@"\n' "$sdk/toolset/bin/ld64.lld" > "$toolchain/bin/ld"
chmod +x "$toolchain/bin/ld"
ln -sfn "$sdk/toolset/bin/libtool" "$toolchain/bin/libtool"
ln -sfn "$sdk/toolset/bin/dsymutil" "$toolchain/bin/dsymutil"
ln -sfn "$sdk/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/clang" "$toolchain/lib/clang"
export ACCOMPANIST_IOS_TOOLCHAIN="$toolchain"
export ACCOMPANIST_IOS_SYSROOT="$(find "$sdk/Developer/Platforms/iPhoneOS.platform/Developer/SDKs" -maxdepth 1 -type d -name 'iPhoneOS*.sdk' | sort -V | tail -1)"
[[ -n "$ACCOMPANIST_IOS_SYSROOT" ]] || { echo 'Missing iPhoneOS SDK.' >&2; exit 1; }
compiler_jar="$native/konan/lib/kotlin-native-compiler-embeddable.jar"
javac -cp "$compiler_jar" -d "$cache/agent-classes" "$root/tools/ios-local/NativeAppleAgent.java" "$root/tools/ios-local/NativeAppleHooks.java"
python3 - "$compiler_jar" "$cache" <<'PY'
import pathlib, sys, zipfile
cache = pathlib.Path(sys.argv[2])
with zipfile.ZipFile(cache / 'native-apple-agent.jar', 'w') as out, zipfile.ZipFile(sys.argv[1]) as source:
    out.writestr('META-INF/MANIFEST.MF', 'Manifest-Version: 1.0\nPremain-Class: NativeAppleAgent\n\n')
    for file in (cache / 'agent-classes').glob('NativeAppleAgent*.class'):
        out.write(file, file.name)
    for name in source.namelist():
        if name.startswith('org/jetbrains/org/objectweb/asm/') and name.endswith('.class'):
            out.writestr(name, source.read(name))
with zipfile.ZipFile(cache / 'native-apple-hooks.jar', 'w') as out:
    out.write(cache / 'agent-classes/NativeAppleHooks.class', 'NativeAppleHooks.class')
PY
export JAVA_TOOL_OPTIONS="${JAVA_TOOL_OPTIONS:-} -javaagent:$cache/native-apple-agent.jar -Dkonan.home=$overlay"

# Gradle still configures Android plugins. Read the Windows SDK path for WSL.
if [[ -z "${ANDROID_HOME:-}" && -f "$root/local.properties" ]]; then
    ANDROID_HOME="$(python3 - "$root/local.properties" <<'PY'
import pathlib, re, sys
value = next((s.split('=', 1)[1].strip() for s in pathlib.Path(sys.argv[1]).read_text().splitlines() if s.startswith('sdk.dir=')), '')
value = value.replace('\\:', ':').replace('\\\\', '/').replace('\\', '/')
if re.match(r'^[A-Za-z]:/', value): value = '/mnt/' + value[0].lower() + value[2:]
print(value)
PY
    )"
fi
export ANDROID_HOME="${ANDROID_HOME:?Set ANDROID_HOME to your Android SDK directory.}"
maven="${ACCOMPANIST_MAVEN_REPOSITORY:-/mnt/e/maven}"
[[ -d "$maven" ]] || { echo "Local Maven repository missing: $maven" >&2; exit 1; }
maven="$(realpath "$maven")"
mirror="$cache/workspace"
mkdir -p "$mirror"
# Only this isolated source mirror is reconciled; exclude build caches from deletion.
[[ "$(realpath "$mirror")" == "$cache/workspace" && "$mirror" != "$root" ]] || exit 1
rsync -a --delete --exclude=.git --exclude=.gradle --exclude=.kotlin --exclude=build \
    --exclude=ref --exclude=.idea --exclude=local.properties --exclude=.build \
    --exclude=Frameworks --exclude=compose-resources --exclude=xtool \
    --exclude='/sample/ios-app/Resources/' "$root/" "$mirror/"
printf 'sdk.dir=%s\n' "$ANDROID_HOME" > "$mirror/local.properties"
gradle=(java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain
    --no-daemon --max-workers=2 -Dorg.gradle.internal.instrumentation.agent=false
    "-Dorg.gradle.jvmargs=-javaagent:$cache/native-apple-agent.jar -Xmx4g -XX:MaxMetaspaceSize=1g"
    "-Pkotlin.native.home=$overlay")

# Build the sibling core's Apple publication when it is absent locally. Its sources
# and Kotlin plugin version are adjusted in a separate mirror, never in ../lyrics-core.
if [[ ! -f "$maven/com/mocharealm/accompanist/lyrics-core-iosarm64/0.5.0/lyrics-core-iosarm64-0.5.0.klib" ]]; then
    core="${ACCOMPANIST_CORE_SOURCE:-$(dirname "$root")/lyrics-core}"
    [[ -f "$core/build.gradle.kts" ]] || { echo 'Missing local lyrics-core iOS dependency and sibling source.' >&2; exit 1; }
    mkdir -p "$cache/core"
    rsync -a --delete --exclude=.git --exclude=.gradle --exclude=.kotlin --exclude=build \
        --exclude=local.properties "$core/" "$cache/core/"
    python3 - "$cache/core/build.gradle.kts" "$version" "$maven" <<'PY'
from pathlib import Path
import re, sys
p = Path(sys.argv[1])
s = re.sub(r'val kotlinVersion = "[^"]+"', f'val kotlinVersion = "{sys.argv[2]}"', p.read_text())
p.write_text(s.replace('file:///E:/maven', Path(sys.argv[3]).as_uri()))
PY
    (cd "$cache/core"; "${gradle[@]}" -PlocalUnsigned=true publishIosArm64PublicationToLocalRepository)
fi
cd "$mirror"
"${gradle[@]}" -PenableIos=true "-PaccompanistMavenRepository=file://$maven" :sample:shared:linkDebugFrameworkIosArm64

python3 "$root/tools/ios-local/package.py" "$mirror"
cd "$mirror/sample/ios-app"
"$xtool_bin" dev build --ipa
# xtool places the IPA in xtool/. Copy the artifact without moving its build cache.
ipa="$(find xtool -type f -name '*.ipa' | sort | head -1)"
[[ -n "$ipa" ]] || { echo 'xtool completed without an IPA artifact.' >&2; exit 1; }
cp "$ipa" "$root/build/ios-local/Accompanist.ipa"
cp -a Frameworks "$root/sample/ios-app/"
resources="$root/sample/ios-app/Resources/compose-resources"
mkdir -p "$resources"
# Reconcile only this generated output; old excluded fonts must not survive a rebuild.
[[ "$(realpath "$resources")" == "$(realpath "$root")/sample/ios-app/Resources/compose-resources" ]] || exit 1
rsync -a --delete Resources/compose-resources/ "$resources/"
echo "Local iOS build complete: $root/build/ios-local/Accompanist.ipa"
