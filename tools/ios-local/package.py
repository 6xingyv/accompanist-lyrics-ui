"""Wrap the static Kotlin framework and copy generated Compose resources for xtool."""
from pathlib import Path
import plistlib
import re
import shutil
import sys

root = Path(sys.argv[1])
app = root / "sample/ios-app"
# Compose aborts during startup when this required frame scheduling key is absent.
with (app / "Info.plist").open("rb") as source:
    if plistlib.load(source).get("CADisableMinimumFrameDurationOnPhone") is not True:
        raise ValueError("iOS Info.plist must enable CADisableMinimumFrameDurationOnPhone")
framework = root / "sample/shared/build/bin/iosArm64/debugFramework/AccompanistSample.framework"
bundle = app / "Frameworks/AccompanistSample.xcframework"
shutil.copytree(framework, bundle / "ios-arm64/AccompanistSample.framework", dirs_exist_ok=True)
with (bundle / "Info.plist").open("wb") as out:
    plistlib.dump({
        "CFBundlePackageType": "XFWK", "XCFrameworkFormatVersion": "1.0",
        "AvailableLibraries": [{"LibraryIdentifier": "ios-arm64",
            "LibraryPath": "AccompanistSample.framework", "SupportedArchitectures": ["arm64"],
            "SupportedPlatform": "ios"}],
    }, out)

# iOS ResourceReader resolves bundle/compose-resources/<generated ResourceItem.path>.
for module in [root / "sample/shared", root / "src"]:
    generated = module / "build/generated/compose/resourceGenerator"
    accessors = generated / "kotlin/commonMainResourceAccessors"
    prefixes = {match for file in accessors.rglob("*.kt")
        for match in re.findall(r'private const val MD: String = "([^"]+)"', file.read_text())}
    prepared = generated / "preparedResources/commonMain/composeResources"
    for prefix in prefixes:
        relative = Path(prefix)
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError(f"Invalid resource prefix: {prefix}")
        destination = app / "Resources/compose-resources" / relative
        # iOS uses its system font. Exclude bundled fonts and remove any copy left by
        # an earlier build so xtool cannot package the 22 MB desktop/Android font.
        fonts = destination / "font"
        if fonts.exists():
            if not fonts.resolve().is_relative_to((app / "Resources/compose-resources").resolve()):
                raise ValueError(f"Font path escaped the app resources: {fonts}")
            shutil.rmtree(fonts)
        shutil.copytree(prepared, destination, dirs_exist_ok=True,
                        ignore=shutil.ignore_patterns("font"))
