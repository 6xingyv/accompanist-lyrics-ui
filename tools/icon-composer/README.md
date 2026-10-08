# Icon Composer contribution workspace

Two independent patches keep application code out of the upstream feature:

- `patches/assetkit.patch`: document validation, preserved SVG assets,
  layer/group/fill lowering, CoreUI 975 serialization, flat legacy PNGs, and
  format/support documentation.
- `patches/xtool.patch`: `.icon` schema acceptance, dependency-injected
  compilation, resource/plist packing, conflict detection, and usage docs.

`patches/xtool-1.16.1.patch` is a local compatibility backport for this machine's
legacy Darwin SDK. It uses the same compiler and packing helpers while retaining
the old SDK manager. It also adopts xtool's current packaged XADI dependency so
the backport builds without a separately installed libxadi. This backport is
not part of the proposed upstream feature.

The working sources are in `ref/AssetKit` and `ref/xtool`. The patches preserve
them in this repository because `ref/` is an ignored research directory. Their
bases are pinned in `build.sh`; it refuses a different checkout or a partially
applied patch rather than overwriting local work.

## Build locally

In WSL/Linux, with Swift 6.3+, git, Python 3, rsync, make, pkg-config, librsvg's
`rsvg-convert`, and xtool's native development dependencies installed:

```bash
bash tools/icon-composer/build.sh
bash tools/ios-local/build.sh
```

The helper applies missing patches and builds both repositories in an isolated
cache. Only the build mirror uses a local AssetKit package dependency. It runs
xtool's required `make lint` and creates a wrapper at
`~/.cache/accompanist-ios/icon-composer/bin/xtool`. The iOS adapter selects this
wrapper when present; the system xtool executable is preserved. Override the
selection with `ACCOMPANIST_XTOOL_BIN`. `ACCOMPANIST_IOS_CACHE` controls both
caches. The wrapper also uses isolated native libraries/rsvg binaries when
available on this machine.

If the installed SDK reports the legacy `develop` format, the helper additionally
builds the 1.16.1 backport and points the wrapper at it. This avoids an unrelated
SDK migration for local device verification. The main-branch contribution patch
is still compiled and linted. No SDK compatibility checks are bypassed and no
installed SDK version markers are rewritten.

Use the native dependency versions from xtool's Dockerfile (libplist,
libimobiledevice-glue, libusbmuxd, libtatsu, libimobiledevice, OpenSSL); old
distribution libimobiledevice headers are insufficient for current xtool.
When changing library versions in an existing mirror, `swift package clean`
may be needed to discard Clang's cached system-library modules.

## Upstream order and limits

Submit/review AssetKit first, publish a release containing `IconCompiler`, then
update xtool's AssetKit requirement to that released version. The xtool patch
currently names the existing 1.0 version line; it cannot compile against an
unmodified release that lacks the new API. The local helper deliberately uses
the sibling checkout while this feature is unpublished.

This is an **experimental** implementation of an observed private file format.
It supports the supplied Accompanist document; unsupported fields cause errors.
Read `ref/AssetKit/docs/icon-composer.md` for the supported subset and remaining
compatibility gates. Mac/watch icon output, catalog merging, refractivity, and
appearance-specific transforms are not implemented. xtool's Xcode generator
explicitly rejects this `.icon` path instead of silently omitting the icon.

Swift compilation and generating a valid-looking BOM are insufficient to call
this release-ready. Apple's assetutil and real device light/dark/tinted checks
must confirm the layered catalog is used, rather than the loose PNG fallback.
Third-party reference catalogs/artwork stay in ignored research folders and
are not included in the patches.

See [release IPA format findings](format-notes.md) for the header/table/descriptor
corrections and the exact verification status. The corrected layered-only
catalog displays the supplied icon on the iPhone home screen; additional
appearance and Apple parser checks are documented there.
