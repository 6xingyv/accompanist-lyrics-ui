import org.jetbrains.compose.desktop.application.dsl.TargetFormat
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.jetbrains.kotlin.jvm)
    alias(libs.plugins.jetbrains.compose)
    alias(libs.plugins.compose.compiler)
}

kotlin {
    jvmToolchain {
        languageVersion.set(JavaLanguageVersion.of(21))
        vendor.set(JvmVendorSpec.JETBRAINS)
    }
    compilerOptions {
        jvmTarget = JvmTarget.JVM_21
    }
}

dependencies {
    implementation(project(":sample:shared"))
    implementation(project(":src"))

    implementation(compose.runtime)
    implementation(compose.foundation)
    implementation(compose.material3)
    implementation(compose.ui)
    implementation(compose.components.resources)
    implementation(compose.desktop.currentOs)
    implementation("org.jetbrains.runtime:jbr-api:1.9.0")

    implementation(libs.accompanist.lyrics.core)
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.11.0")
    implementation("com.github.hypfvieh:dbus-java-core:5.2.2")
    runtimeOnly("com.github.hypfvieh:dbus-java-transport-native-unixsocket:5.2.2")
    implementation(platform(libs.koin.bom))
    implementation(libs.koin.compose)
    testImplementation(kotlin("test"))
}

val desktopJbr = javaToolchains.launcherFor {
    languageVersion.set(JavaLanguageVersion.of(21))
    vendor.set(JvmVendorSpec.JETBRAINS)
}
// Android Studio's JBR includes javac but omits JNI headers, so even compilerFor
// can select it. Resolve a separate OpenJDK SDK for the standard JNI headers;
// application launch and window integration still use JetBrains Runtime.
val desktopJniJdk = javaToolchains.compilerFor {
    languageVersion.set(JavaLanguageVersion.of(21))
    vendor.set(JvmVendorSpec.AZUL)
}
val nativeOs = when {
    System.getProperty("os.name").startsWith("Windows") -> "windows"
    System.getProperty("os.name").startsWith("Mac") -> "macos"
    else -> "linux"
}
val nativeArch = when (System.getProperty("os.arch").lowercase()) {
    "amd64", "x86_64" -> "x64"
    "aarch64", "arm64" -> "arm64"
    else -> error("Unsupported desktop architecture: ${System.getProperty("os.arch")}")
}
val nativeResources = layout.buildDirectory.dir("generated/native-resources")
val nativeLibrary = nativeResources.map {
    it.file("native/$nativeOs-$nativeArch/${System.mapLibraryName("accompanist_media")}")
}
val buildNativeMedia by tasks.registering {
    group = "build"
    description = "Build the in-process Windows/macOS media JNI adapter"
    inputs.dir(layout.projectDirectory.dir("native"))
    inputs.property("platform", "$nativeOs-$nativeArch")
    if (nativeOs != "linux") {
        inputs.property("jniJdk", desktopJniJdk.map { it.metadata.installationPath.asFile.absolutePath })
    }
    outputs.file(nativeLibrary)
    onlyIf { nativeOs != "linux" }
    doLast {
        val jdk = desktopJniJdk.get().metadata.installationPath.asFile
        val jniPlatform = if (nativeOs == "windows") "win32" else "darwin"
        check(File(jdk, "include/jni.h").isFile && File(jdk, "include/$jniPlatform/jni_md.h").isFile) {
            "JNI headers were not found in the native build JDK: $jdk"
        }
        val output = nativeLibrary.get().asFile.apply { parentFile.mkdirs() }
        val work = layout.buildDirectory.dir("native/$nativeOs-$nativeArch").get().asFile.apply { mkdirs() }
        val native = layout.projectDirectory.dir("native").asFile
        val command = if (nativeOs == "windows") {
            val vswhere = File(System.getenv("ProgramFiles(x86)"), "Microsoft Visual Studio/Installer/vswhere.exe")
            check(vswhere.isFile) { "Install Visual Studio C++ tools and the Windows SDK to build the native SMTC adapter" }
            val discover = ProcessBuilder(vswhere.path, "-latest", "-products", "*", "-requires",
                "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath").start()
            val installation = discover.inputStream.bufferedReader().readText().trim()
            check(discover.waitFor() == 0 && installation.isNotEmpty()) { "Visual Studio C++ tools were not found" }
            val vcvars = File(installation, "VC/Auxiliary/Build/vcvarsall.bat")
            val includes = File(System.getenv("ProgramFiles(x86)"), "Windows Kits/10/Include")
            val sdk = includes.listFiles()?.filter {
                File(it, "cppwinrt/winrt/Windows.Media.Control.h").isFile
            }?.maxByOrNull { it.name } ?: error("Windows SDK C++/WinRT headers were not found")
            val script = File(work, "compile.cmd")
            script.writeText("""
                @echo off
                call "${vcvars.path}" ${if (nativeArch == "arm64") "amd64_arm64" else "x64"} >nul
                if errorlevel 1 exit /b 1
                cl /nologo /O2 /std:c++20 /EHsc /MT /LD /I"${File(jdk, "include")}" /I"${File(jdk, "include/win32")}" /I"${File(sdk, "cppwinrt")}" "${File(native, "windows_media.cpp")}" /Fe:"${output.path}" /link windowsapp.lib /IMPLIB:"${File(work, "accompanist_media.lib")}" /PDB:"${File(work, "accompanist_media.pdb")}" /INCREMENTAL:NO
                exit /b %errorlevel%
            """.trimIndent())
            listOf("cmd.exe", "/d", "/c", script.path)
        } else {
            listOf("/usr/bin/clang++", "-std=c++20", "-O2", "-fobjc-arc", "-dynamiclib",
                "-arch", if (nativeArch == "arm64") "arm64" else "x86_64",
                "-mmacosx-version-min=12.0", "-I${File(jdk, "include")}", "-I${File(jdk, "include/darwin")}",
                File(native, "macos_media.mm").path, "-framework", "Foundation", "-framework", "AppKit",
                "-framework", "ScriptingBridge", "-o", output.path)
        }
        val compile = ProcessBuilder(command).directory(work).redirectErrorStream(true).start()
        compile.inputStream.bufferedReader().forEachLine { logger.lifecycle(it) }
        check(compile.waitFor() == 0) {
            "Native desktop media compilation failed"
        }
    }
}
sourceSets.main { resources.srcDir(nativeResources) }
tasks.processResources { dependsOn(buildNativeMedia) }

compose {
    resources {
        packageOfResClass = "com.mocharealm.accompanist.sample.desktop.resources"
    }
    desktop {
        application {
            mainClass = "com.mocharealm.accompanist.sample.MainKt"
            javaHome = desktopJbr.get().metadata.installationPath.asFile.absolutePath

            nativeDistributions {
                modules("java.desktop", "java.net.http", "java.prefs", "jdk.unsupported")
                targetFormats(TargetFormat.Dmg, TargetFormat.Exe)
                packageName = "Accompanist"
                packageVersion = "1.0.0"
                macOS {
                    bundleID = "com.mocharealm.accompanist.desktop"
                    minimumSystemVersion = "12.0"
                    entitlementsFile.set(layout.projectDirectory.file("packaging/macos-entitlements.plist"))
                    runtimeEntitlementsFile.set(layout.projectDirectory.file("packaging/macos-entitlements.plist"))
                    infoPlist {
                        extraKeysRawXml = """
                            <key>NSAppleEventsUsageDescription</key>
                            <string>Accompanist reads the current song and seeks Music or Spotify when you click lyrics.</string>
                        """.trimIndent()
                    }
                }
            }
        }
    }
}

composeCompiler {
    stabilityConfigurationFiles.add(rootProject.layout.projectDirectory.file("compose_compiler_config.conf"))
}
