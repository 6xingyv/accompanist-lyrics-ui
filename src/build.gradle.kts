import com.android.build.api.dsl.androidLibrary
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
import javax.inject.Inject

plugins {
    alias(libs.plugins.jetbrains.kotlin.multiplatform)
    alias(libs.plugins.jetbrains.compose)
    alias(libs.plugins.android.kotlin.multiplatform.library)
    alias(libs.plugins.compose.compiler)
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
    androidLibrary {
        namespace = "com.mocharealm.accompanist.lyrics.ui"
        compileSdk = 36

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
            compilerOptions.configure {
                jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_21
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

                implementation(libs.gaze.capsule)

                implementation(libs.accompanist.lyrics.core)

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
    val configFile = project.layout.projectDirectory.file("compose-compiler-config.conf")
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

    @get:OutputDirectory
    abstract val jniLibsDir: DirectoryProperty

    @TaskAction
    fun build() {
        val abiMap = mapOf(
            "arm64-v8a" to "aarch64-linux-android",
            "x86_64" to "x86_64-linux-android"
        )

        abiMap.forEach { (abi, target) ->
            // Ensure output directory exists
            val outputDir = jniLibsDir.get().dir(abi).asFile
            if (!outputDir.exists()) {
                outputDir.mkdirs()
            }

            execOps.exec {
                workingDir = rustProjectDir.get().asFile
                commandLine("cargo", "ndk", "-t", abi, "build", "--release")
            }.assertNormalExitValue()

            fs.copy {
                from(rustProjectDir.get().dir("target/$target/release"))
                include("lib${libName.get()}.so")
                into(jniLibsDir.get().dir(abi))
            }
        }
    }
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
