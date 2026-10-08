# Release IPA comparison

Inspected on 2026-10-07. These are observations of downloaded release artifacts,
not an Apple format specification. IPA files, catalogs, and artwork remain in
the ignored `build/ios-local/icon-research/ipas/` directory. Only these findings
are included with the contribution workspace.

## Samples

| Release | CoreUI | Layered icon evidence |
| --- | --- | --- |
| [StikDebug 3.1.13](https://github.com/StikDebug/StikDebug/releases/tag/3.1.13) | 975 | Primary icon: 3 stack, 6 group, 2 SVG renditions |
| [Feather 2.9.0](https://github.com/claration/feather/releases/tag/v2.9.0) | 973 | Primary icon: 3 stack, 3 group renditions; raster layer sources |
| [Apollo Reborn 3.8.5, Glass Icons, no extensions](https://github.com/Apollo-Reborn/Apollo-Reborn/releases/tag/v1.15.11_3.8.5) | 975 | Primary icon family: 2 stack, 11 group, 8 SVG renditions; many alternate icons |
| [LiveContainer 3.8.0](https://github.com/LiveContainer/LiveContainer/releases/tag/3.8.0) | — | No stack/group renditions; ordinary asset catalog control |

Counts include appearance variants. Apollo is a modified application, so its
original Info.plist SDK metadata does not describe the replacement catalog.
StikDebug and Feather provide independent comparisons for the layered records.

Downloaded IPA SHA-256:

```text
StikDebug     d730fdfde1aeb3d1d3cc0cc71404f85fb9eb295d38876e2bf668a5d630f20e8c
Feather       833c57d3c4bf4b4a9fe408be72e1c720cbb949f736a00286750f50e7915730ea
Apollo        75aa3de0cffb52b4c9634dac1559b1ea0e1b5912f604a8b05966c0d115c8912c
LiveContainer b6fea95e30083382e29ffef88fa1aaa40b5069e1112e5307d490dab04648bba6
```

## Bundle resources

The layered samples store their compiled icon in the application root's
`Assets.car`. Primary icon declarations use
`CFBundleIcons.CFBundlePrimaryIcon.CFBundleIconName` and the equivalent iPad
dictionary. They also contain loose PNGs for older systems. No raw `.icon`
directory is required in the installed bundle. A top-level `CFBundleIconName`
is not present in every sample.

This confirms that writing loose PNGs alone cannot verify the layered catalog:
device verification must use an IPA with those fallback declarations/resources
removed. The compiler's normal output retains legacy fallbacks.

## CSI dimensions and scale

For all observed stack/group renditions in these three primary icon families:

| CSI field | Stack, layout 1019 | Group, layout 1020 | SVG, layout 9 |
| --- | --- | --- | --- |
| Width, byte offset 12 | 1024 | 0 | 0 |
| Height, byte offset 16 | 1024 | 0 | 0 |
| Scale factor, byte offset 20 | 100 | 100 | 0 |

The stack/group rendition lookup key still has scale token 1. Header scale
factor and key scale token are separate encodings. The initial implementation
incorrectly reused the zero dimensions/scale of SVG and named-fill headers for
layer nodes. The corrected writer now emits the values above.

These fields now match the release artifacts. The earlier background-only
result came from the USB SpringBoard PNG API; see the device findings below
before interpreting that result as the home screen's rendering.

## Layer table framing

TLVs 1012, 1020, and 1021 start with a UInt32 count and a reserved UInt32 zero.
Their entry layouts are:

- **1012:** 48 bytes per entry: geometry, blend, opacity, and sparse asset key.
- **1020:** UInt32 group-lighting/image-glass selector, Float blur, UInt32 fill
  string byte length, then UTF-8 bytes including the terminating NUL.
- **1021:** 20 bytes per entry: UInt32 specular, Float translucency,
  UInt32 shadow kind, Float shadow intensity, UInt32 reserved zero.

All fields here are little endian. The lighting table's fill string makes
entry lengths variable; group and image entries have the same framing.

The initial writer treated the table-level reserved word as an extra field in
each image entry. With one image, the encoded bytes coincidentally matched. A
second image received four extra bytes in both lighting and material tables.
The corrected writer emits each table header once and then its entries.

The corrected framing consumes all bytes of 28 stack/group records across the
three primary icon families, including multi-image and multi-group cases.

## Resource descriptor and key format consistency

The three iOS samples use ten KEYFORMAT attributes. Their BITMAPKEYS descriptors
have four UInt32 header words followed by ten attribute descriptor words,
56 bytes total. Header words 2 and 3 are 44 and 10 respectively. The initial
implementation copied a nine-attribute macOS descriptor (52 bytes, header
40/9) into a catalog with ten-attribute rendition keys.

The corrected writer uses the observed iOS descriptor. It defines attribute
order and descriptor tokens together in `IconKeyFormat`: rendition key
generation, KEYFORMAT metadata, descriptor count, and descriptor size all use
that definition. The existing XCAsset compiler's schema remains separate.

## Other observations and limits

- StikDebug has a glass SVG layer with an empty fill name; an explicit white
  replacement fill is not required by this encoding.
- StikDebug's CAR header UUID is zero. A zero UUID alone is not evidence of an
  invalid layered catalog.
- KEYFORMAT varies: StikDebug/Feather use attribute 9 where Apollo uses 8;
  root BITMAPKEYS descriptors also vary with the catalog's extra renditions.
  The compiler follows the observed Apollo CoreUI 975 generic asset descriptor
  profile. Broader compatibility still needs review.
- Release JSON sources also contain hidden layers, empty editor groups, and
  appearance-specific material settings. The current strict compiler rejects
  unsupported fields; the presence of these samples does not imply complete
  support for compiling their source documents.

## Current verification state

Swift compilation, xtool's required `make lint`, and lint of the six new
AssetKit Swift files pass. The corrected sample IPA builds successfully and
its three stack/group appearance pairs have the dimensions and scales above.
The sample icon's JSON has been restored to the supplied original, without
the temporary transparency/fill changes used during diagnosis.

On the connected iPhone 16,1 running iOS 26.7, the corrected layered-only IPA
was installed with all four fallback PNGs and their plist declarations removed.
The user confirmed that the home screen shows the blue background and white
bass clef correctly. The catalog contains SVG sources and layered records,
with no raster icon renditions; this confirms the layered catalog is loaded.

The USB SpringBoard icon PNG API is not a reliable check of this rendering:
an unchanged Xcode-produced StikDebug catalog also returned the placeholder
grid through that API, while the user confirmed the home screen showed its
blue lightning icon correctly. Earlier USB-only background/placeholder results
therefore cannot establish how the home screen rendered those versions.

The user also confirmed that dark and tinted home screen appearances render
correctly. The normal IPA containing legacy PNG fallbacks was then installed
successfully. Its CAR is byte-for-byte identical to the device-verified catalog:

```text
SHA-256 a995fa629f03f3aa5c467b5931a6ac9e8045df53d32243dac13d767e5f7ee032
```

Apple's `assetutil` validation and compatibility beyond this supplied SVG icon,
device, and OS version remain pending. This verification does not establish
support for fields or source formats the compiler currently rejects.
