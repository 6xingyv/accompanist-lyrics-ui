// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "AccompanistIos",
    platforms: [.iOS(.v15)],
    products: [.library(name: "AccompanistIos", targets: ["AccompanistIos"])],
    targets: [
        .binaryTarget(name: "AccompanistSample", path: "Frameworks/AccompanistSample.xcframework"),
        .target(name: "AccompanistIos", dependencies: ["AccompanistSample"],
            linkerSettings: [.unsafeFlags(["-Xlinker", "-ObjC"])])
    ]
)
