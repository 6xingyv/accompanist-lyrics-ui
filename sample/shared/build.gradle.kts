import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.jetbrains.kotlin.multiplatform)
    alias(libs.plugins.jetbrains.compose)
    alias(libs.plugins.android.kotlin.multiplatform.library)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.stability.analyzer)
}

kotlin {
    compilerOptions { freeCompilerArgs.add("-Xexpect-actual-classes") }
    android {
        namespace = "com.mocharealm.accompanist.sample.shared"
        compileSdk = 37
        minSdk = 29

        androidResources {
            enable = true
        }

        compilations.configureEach {
            compileTaskProvider.configure {
                compilerOptions {
                    jvmTarget.set(JvmTarget.JVM_21)
                }
            }
        }
    }

    jvm {
        compilerOptions {
            jvmTarget = JvmTarget.JVM_21
        }
    }

    if (System.getProperty("os.name") == "Mac OS X" ||
        providers.gradleProperty("enableIos").map(String::toBoolean).getOrElse(false)) {
        iosArm64 {
            binaries.framework {
                baseName = "AccompanistSample"
                isStatic = true
                // Apple's transparent stepping extension is absent from Linux LLVM.
                if (System.getProperty("os.name") == "Linux") {
                    freeCompilerArgs += "-Xbinary=enableDebugTransparentStepping=false"
                }
            }
        }
    }

    sourceSets {
        commonMain.dependencies {
            implementation(compose.runtime)
            implementation(compose.foundation)
            implementation(compose.material3)
            implementation(compose.ui)
            implementation(compose.components.resources)
            implementation(compose.components.uiToolingPreview)

            implementation(libs.accompanist.lyrics.core)

            implementation(libs.gaze.capsule)

            implementation(project.dependencies.platform(libs.koin.bom))
            implementation(libs.koin.compose)
            implementation(libs.koin.compose.viewmodel)

            implementation(project(":src"))
        }

        androidMain.dependencies {
            implementation(libs.androidx.activity.compose)
        }

        commonTest.dependencies { implementation(kotlin("test")) }

        jvmMain.dependencies {
            implementation(compose.desktop.currentOs)
        }
    }
}

compose {
    resources {
        packageOfResClass = "com.mocharealm.accompanist.sample"
        publicResClass = true
    }
}

composeCompiler {
    stabilityConfigurationFiles.add(rootProject.layout.projectDirectory.file("compose_compiler_config.conf"))
}

tasks.named("stabilityCheck") {
    dependsOn("compileAndroidMain", "compileKotlinJvm")
}
