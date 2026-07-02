import com.vanniktech.maven.publish.JavadocJar
import com.vanniktech.maven.publish.KotlinMultiplatform
import org.gradle.kotlin.dsl.support.serviceOf
import org.gradle.api.DefaultTask
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.FileSystemOperations
import org.gradle.api.provider.Property
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputDirectory
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.TaskAction
import org.gradle.process.ExecOperations
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import java.io.File
import java.util.Properties
import javax.inject.Inject

plugins {
    alias(libs.plugins.jetbrains.kotlin.multiplatform)
    alias(libs.plugins.jetbrains.compose)
    alias(libs.plugins.android.kotlin.multiplatform.library)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.kotlinx.serialization)
    alias(libs.plugins.maven.publish)
    alias(libs.plugins.dokka)
}

val rustDir = layout.projectDirectory.dir("../text_engine")
val rustLibName = "text_engine"

kotlin {
    targets.all {
        compilations.all {
            compileTaskProvider.configure {
                compilerOptions {
                    freeCompilerArgs.add("-Xexpect-actual-classes")
                }
            }
        }
    }
    android {
        namespace = "com.mocharealm.accompanist.lyrics.ui"
        compileSdk = 37

        minSdk = 29

        optimization {
            consumerKeepRules.apply {
                publish = true
                file("consumer-rules.pro")
            }
        }

        withJava()
        withHostTestBuilder {}.configure {}
        withDeviceTestBuilder {
            sourceSetTreeName = "test"
        }

        compilations.configureEach {
            compileTaskProvider.configure {
                compilerOptions {
                    jvmTarget.set(JvmTarget.JVM_21)
                }
            }
        }
    }
    jvm()
    val isMac = org.gradle.internal.os.OperatingSystem.current().isMacOsX
    if (isMac) {
        val appleTargets = listOf(
            iosArm64(),
            iosSimulatorArm64(),
            macosArm64()
        )
        appleTargets.forEach { target ->
            target.compilations.getByName("main") {
                val myInterop by cinterops.creating {
                    defFile(project.file("src/nativeInterop/cinterop/$rustLibName.def"))
                    packageName = "com.mocharealm.accompanist.lyrics.ui.native"
                }
            }
        }
    }

    sourceSets {
        commonMain {
            dependencies {
                implementation(compose.runtime)
                implementation(compose.foundation)
                implementation(compose.material3)
                implementation(compose.ui)
                implementation(compose.components.uiToolingPreview)

                implementation(libs.accompanist.lyrics.core)
                implementation(libs.kotlinx.serialization.json)

                implementation(compose.components.resources)
            }
        }
        androidMain.dependencies {
            implementation(libs.hiddenapibypass)
        }
    }
}

publishing {
    repositories {
        maven {
            name = "local"
            url = uri("file:///E:/maven")
        }
    }
}

mavenPublishing {
    publishToMavenCentral(automaticRelease = true)

    configure(
        KotlinMultiplatform(
            javadocJar = JavadocJar.Dokka("dokkaGeneratePublicationHtml"),
            sourcesJar = true
        )
    )

    signAllPublications()

    coordinates("com.mocharealm.accompanist", "lyrics-ui", rootProject.version.toString())

    pom {
        name = "Accompanist Lyrics UI"
        description = "A lyrics displaying library for Compose Multiplatform"
        inceptionYear = "2025"
        url = "https://mocharealm.com/open-source"
        licenses {
            license {
                name = "The Apache License, Version 2.0"
                url = "http://www.apache.org/licenses/LICENSE-2.0.txt"
                distribution = "http://www.apache.org/licenses/LICENSE-2.0.txt"
            }
        }
        developers {
            developer {
                id = "6xingyv"
                name = "Simon Scholz"
                url = "https://github.com/6xingyv"
            }
        }
        scm {
            url = "https://github.com/6xingyv/Accompanist"
            connection = "scm:git:git://github.com/6xingyv/Accompanist.git"
            developerConnection = "scm:git:ssh://git@github.com/6xingyv/Accompanist.git"
        }
    }
}

composeCompiler {
    val configFile = rootProject.layout.projectDirectory.file("compose_compiler_config.conf")
    if (configFile.asFile.exists()) {
        stabilityConfigurationFiles.add(configFile)
    }
}

abstract class BuildRustAndroidTask @Inject constructor(
    private val execOps: ExecOperations,
    private val fs: FileSystemOperations
) : DefaultTask() {
    @get:InputDirectory
    abstract val rustProjectDir: DirectoryProperty

    @get:Input
    abstract val libName: Property<String>

    @get:Input
    abstract val androidApiLevel: Property<Int>

    @get:OutputDirectory
    abstract val jniLibsDir: DirectoryProperty

    @TaskAction
    fun build() {
        val abiMap = mapOf(
            "arm64-v8a" to "aarch64-linux-android",
            "armeabi-v7a" to "armv7-linux-androideabi",
            "x86_64" to "x86_64-linux-android"
        )
        val androidNdkDir = resolveAndroidNdkDir()
        val skiaNinjaCommand = resolveSkiaNinjaCommand()
        val libclangPath = resolveLibclangPath(androidNdkDir)

        abiMap.forEach { (abi, target) ->
            // Ensure output directory exists
            val outputDir = jniLibsDir.get().dir(abi).asFile
            if (!outputDir.exists()) {
                outputDir.mkdirs()
            }

            execOps.exec {
                workingDir = rustProjectDir.get().asFile
                commandLine(
                    "cargo",
                    "ndk",
                    "-t", abi,
                    "-P", androidApiLevel.get().toString(),
                    "build",
                    "--release"
                )
                environment("ANDROID_NDK", androidNdkDir.absolutePath)
                environment("ANDROID_NDK_HOME", androidNdkDir.absolutePath)
                environment("ANDROID_NDK_ROOT", androidNdkDir.absolutePath)
                environment.remove("SDKROOT")
                environment.remove("SDKTARGETSYSROOT")
                skiaNinjaCommand?.let { environment("SKIA_NINJA_COMMAND", it.absolutePath) }
                libclangPath?.let { environment("LIBCLANG_PATH", it.absolutePath) }
            }.assertNormalExitValue()

            fs.copy {
                from(rustProjectDir.get().dir("target/$target/release"))
                include("lib${libName.get()}.so")
                into(jniLibsDir.get().dir(abi))
            }
        }
    }

    private fun resolveAndroidNdkDir(): File {
        listOf("ANDROID_NDK", "ANDROID_NDK_HOME", "ANDROID_NDK_ROOT").forEach { name ->
            System.getenv(name)
                ?.takeIf { it.isNotBlank() }
                ?.let { File(it) }
                ?.takeIf { it.isDirectory }
                ?.let { return it }
        }

        localPropertiesFiles().forEach { propertiesFile ->
            val properties = Properties()
            propertiesFile.inputStream().use { properties.load(it) }

            properties.getProperty("ndk.dir")
                ?.takeIf { it.isNotBlank() }
                ?.let { File(it) }
                ?.takeIf { it.isDirectory }
                ?.let { return it }

            properties.getProperty("sdk.dir")
                ?.takeIf { it.isNotBlank() }
                ?.let { File(it) }
                ?.let { sdkDir -> findLatestNdkInSdk(sdkDir) }
                ?.let { return it }
        }

        listOf("ANDROID_HOME", "ANDROID_SDK_ROOT").forEach { name ->
            System.getenv(name)
                ?.takeIf { it.isNotBlank() }
                ?.let { File(it) }
                ?.let { sdkDir -> findLatestNdkInSdk(sdkDir) }
                ?.let { return it }
        }

        throw org.gradle.api.GradleException(
            "Android NDK not found. Set ANDROID_NDK or add ndk.dir/sdk.dir to local.properties."
        )
    }

    private fun localPropertiesFiles(): List<File> =
        listOf(
            project.rootProject.file("local.properties"),
            project.file("local.properties")
        ).distinct().filter { it.isFile }

    private fun findLatestNdkInSdk(sdkDir: File): File? {
        val ndkRoot = File(sdkDir, "ndk")
        val ndks = ndkRoot.listFiles { file ->
            file.isDirectory && File(file, "source.properties").isFile
        } ?: return null

        return ndks.maxWithOrNull { left, right ->
            compareVersionNames(left.name, right.name)
        }
    }

    private fun compareVersionNames(left: String, right: String): Int {
        val leftParts = left.split('.', '-').map { it.toIntOrNull() ?: 0 }
        val rightParts = right.split('.', '-').map { it.toIntOrNull() ?: 0 }
        repeat(maxOf(leftParts.size, rightParts.size)) { index ->
            val diff = leftParts.getOrElse(index) { 0 } - rightParts.getOrElse(index) { 0 }
            if (diff != 0) return diff
        }
        return left.compareTo(right)
    }

    private fun resolveSkiaNinjaCommand(): File? {
        System.getenv("SKIA_NINJA_COMMAND")
            ?.takeIf { it.isNotBlank() }
            ?.let { File(it) }
            ?.takeIf { it.isFile }
            ?.let { return it }

        findExecutableInPath("ninja")?.let { return it }

        val executableName = if (isWindows()) "ninja.exe" else "ninja"
        return listOf(
            project.rootProject.file("../skiko/skia/third_party/ninja/$executableName"),
            project.rootProject.file("../skiko/skiko/skia/third_party/ninja/$executableName"),
            project.rootProject.file("../../skiko/skia/third_party/ninja/$executableName"),
            project.rootProject.file("../../skiko/skiko/skia/third_party/ninja/$executableName")
        ).firstOrNull { it.isFile }
    }

    private fun resolveLibclangPath(androidNdkDir: File): File? {
        System.getenv("LIBCLANG_PATH")
            ?.takeIf { it.isNotBlank() }
            ?.let { File(it) }
            ?.let { path ->
                if (path.isFile) path.parentFile else path
            }
            ?.takeIf { it.isDirectory }
            ?.let { return it }

        val pathLibclang = findFileInPath(libclangLibraryNames())

        if (isWindows()) {
            val localAppData = System.getenv("LOCALAPPDATA")?.let { File(it) }
            val swiftToolchains = localAppData?.resolve("Programs/Swift/Toolchains")
            swiftToolchains?.listFiles { file -> file.isDirectory }
                ?.asSequence()
                ?.map { it.resolve("usr/bin") }
                ?.firstOrNull { dir -> libclangLibraryNames().any { dir.resolve(it).isFile } }
                ?.let { return it }

            listOf(
                File("C:/Program Files/LLVM/bin"),
                File("C:/Program Files (x86)/LLVM/bin")
            ).firstOrNull { dir ->
                libclangLibraryNames().any { dir.resolve(it).isFile }
            }?.let { return it }

            pathLibclang
                ?.takeUnless { isUnderDirectory(it, androidNdkDir) }
                ?.parentFile
                ?.let { return it }
        } else {
            pathLibclang?.parentFile?.let { return it }
        }

        findLibclangUnderNdk(androidNdkDir)?.let { return it }

        return null
    }

    private fun findLibclangUnderNdk(androidNdkDir: File): File? {
        val prebuiltRoot = androidNdkDir.resolve("toolchains/llvm/prebuilt")
        return prebuiltRoot.listFiles { file -> file.isDirectory }
            ?.asSequence()
            ?.map { it.resolve("bin") }
            ?.firstOrNull { dir -> libclangLibraryNames().any { dir.resolve(it).isFile } }
    }

    private fun findFileInPath(fileNames: List<String>): File? {
        val path = System.getenv("PATH") ?: return null
        return path.split(File.pathSeparator)
            .asSequence()
            .map { File(it) }
            .filter { it.isDirectory }
            .flatMap { dir -> fileNames.asSequence().map { fileName -> File(dir, fileName) } }
            .firstOrNull { it.isFile }
    }

    private fun findExecutableInPath(command: String): File? {
        val path = System.getenv("PATH") ?: return null
        val extensions = if (isWindows()) listOf(".exe", ".cmd", ".bat", "") else listOf("")
        return path.split(File.pathSeparator)
            .asSequence()
            .map { File(it) }
            .filter { it.isDirectory }
            .flatMap { dir ->
                extensions.asSequence().map { extension ->
                    if (command.endsWith(extension, ignoreCase = true)) {
                        File(dir, command)
                    } else {
                        File(dir, "$command$extension")
                    }
                }
            }
            .firstOrNull { it.isFile }
    }

    private fun libclangLibraryNames(): List<String> = when {
        isWindows() -> listOf("libclang.dll", "clang.dll")
        System.getProperty("os.name").lowercase().contains("mac") -> listOf("libclang.dylib")
        else -> listOf("libclang.so")
    }

    private fun isUnderDirectory(file: File, directory: File): Boolean {
        val filePath = file.absoluteFile.normalize().path.lowercase()
        val directoryPath = directory.absoluteFile.normalize().path.lowercase()
        return filePath == directoryPath || filePath.startsWith("$directoryPath${File.separator}")
    }

    private fun isWindows(): Boolean =
        System.getProperty("os.name").lowercase().contains("win")
}

abstract class BuildRustJvmTask @Inject constructor(
    private val execOps: ExecOperations,
    private val fs: FileSystemOperations
) : DefaultTask() {
    @get:InputDirectory
    abstract val rustProjectDir: DirectoryProperty

    @get:Input
    abstract val libName: Property<String>

    @get:OutputDirectory
    abstract val resourcesDir: DirectoryProperty

    @TaskAction
    fun build() {
        execOps.exec {
            workingDir = rustProjectDir.get().asFile
            commandLine("cargo", "build", "--release")
        }.assertNormalExitValue()

        val osName = System.getProperty("os.name").lowercase()
        val (ext, prefix) = when {
            osName.contains("win") -> "dll" to ""
            osName.contains("mac") -> "dylib" to "lib"
            else -> "so" to "lib"
        }
        val osId = when {
            osName.contains("win") -> "windows"
            osName.contains("mac") -> "macos"
            else -> "linux"
        }
        val archName = System.getProperty("os.arch").lowercase()
        val archId = when (archName) {
            "amd64", "x86_64" -> "x86_64"
            "aarch64", "arm64" -> "arm64"
            else -> archName
        }

        // Ensure output directory exists
        val outputDir = resourcesDir.get().dir("natives/$osId-$archId").asFile
        if (!outputDir.exists()) {
            outputDir.mkdirs()
        }

        fs.copy {
            from(rustProjectDir.get().dir("target/release"))
            include("$prefix${libName.get()}.$ext")
            into(resourcesDir.get().dir("natives/$osId-$archId"))
        }
    }
}

abstract class BuildRustAppleTask @Inject constructor(
    private val execOps: ExecOperations
) : DefaultTask() {
    @get:InputDirectory
    abstract val rustProjectDir: DirectoryProperty

    @get:Input
    abstract val libName: Property<String>

    @get:OutputDirectory
    abstract val headerOutputDir: DirectoryProperty

    @TaskAction
    fun build() {
        val appleTargets = listOf(
            "aarch64-apple-ios",
            "aarch64-apple-ios-sim",
            "aarch64-apple-darwin"
        )

        // 1. Compile static libraries for each platform
        appleTargets.forEach { target ->
            execOps.exec {
                workingDir = rustProjectDir.get().asFile
                commandLine("cargo", "build", "--target", target, "--release")
            }.assertNormalExitValue()
        }

        // 2. Generate C header interface
        val headerOut = headerOutputDir.get().asFile
        if (!headerOut.exists()) {
            headerOut.mkdirs()
        }
        val headerFile = headerOutputDir.get().file("${libName.get()}.h").asFile

        execOps.exec {
            workingDir = rustProjectDir.get().asFile
            commandLine(
                "cbindgen",
                "--config", "cbindgen.toml",
                "--crate", libName.get(),
                "--output", headerFile.absolutePath
            )
        }.assertNormalExitValue()
    }
}

val buildRustAndroid = tasks.register<BuildRustAndroidTask>("buildRustAndroid") {
    rustProjectDir.set(rustDir)
    libName.set(rustLibName)
    androidApiLevel.set(29)
    jniLibsDir.set(layout.projectDirectory.dir("src/androidMain/jniLibs"))
}

val buildRustJvm = tasks.register<BuildRustJvmTask>("buildRustJvm") {
    rustProjectDir.set(rustDir)
    libName.set(rustLibName)
    resourcesDir.set(layout.projectDirectory.dir("src/jvmMain/resources"))
}

val buildRustApple = tasks.register<BuildRustAppleTask>("buildRustApple") {
    // Run only on macOS
    onlyIf { org.gradle.internal.os.OperatingSystem.current().isMacOsX }

    rustProjectDir.set(rustDir)
    libName.set(rustLibName)
    // Output to the standard cinterop directory
    headerOutputDir.set(layout.projectDirectory.dir("src/nativeInterop/cinterop"))
}

tasks.matching { it.name.contains("preBuild", ignoreCase = true) }.configureEach {
    dependsOn(buildRustAndroid)
}

tasks.named("jvmProcessResources") {
    dependsOn(buildRustJvm)
}

tasks.named<Delete>("clean") {
    delete(
        layout.projectDirectory.dir("src/androidMain/jniLibs"),
        layout.projectDirectory.dir("src/jvmMain/resources/natives"),
        layout.projectDirectory.dir("src/nativeInterop/cinterop")
    )
}

tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinNativeCompile>().configureEach {
    dependsOn(buildRustApple)
}

kotlin.targets.withType<org.jetbrains.kotlin.gradle.plugin.mpp.KotlinNativeTarget> {
    binaries.all {
        val rustTarget = when (konanTarget.name) {
            "ios_arm64" -> "aarch64-apple-ios"
            "ios_simulator_arm64" -> "aarch64-apple-ios-sim"
            "macos_arm64" -> "aarch64-apple-darwin"
            else -> null
        }

        rustTarget?.let {
            // Use relative path or provider to be configuration-cache friendly
            val rustLibDir = rustDir.dir("target/$it/release").asFile.absolutePath
            linkerOpts("-L$rustLibDir", "-l$rustLibName")
        }
    }
}
