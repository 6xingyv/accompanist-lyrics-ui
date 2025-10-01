import org.jetbrains.compose.desktop.application.dsl.TargetFormat
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import java.util.Properties

plugins {
    alias(libs.plugins.jetbrains.kotlin.multiplatform)
    alias(libs.plugins.jetbrains.compose)
    alias(libs.plugins.android.application)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.maven.publish)
}

kotlin {
    androidTarget {
        compilerOptions {
            jvmTarget = JvmTarget.JVM_21
        }
    }

    jvm() {
        compilerOptions {
            jvmTarget = JvmTarget.JVM_21
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

            implementation(project.dependencies.platform(libs.koin.bom))
            implementation(libs.koin.compose)
            implementation(libs.koin.compose.viewmodel)

            implementation(project(":src"))
        }

        androidMain.dependencies {
            implementation(libs.androidx.activity.compose)
            implementation(libs.androidx.core.ktx)
            implementation(libs.androidx.lifecycle.runtime.ktx)
            implementation(libs.androidx.media3.exoplayer)
            implementation(libs.androidx.media3.session)
            implementation(libs.cloudy)

            implementation(libs.androidx.lifecycle.viewmodel.compose)
            implementation(libs.kotlinx.coroutines.guava)
        }

        jvmMain.dependencies {
            implementation(compose.desktop.currentOs)
        }
    }
}

android {
    namespace = "com.mocharealm.accompanist.sample"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.mocharealm.accompanist.demo"
        minSdk = 29
        targetSdk = 36
        versionCode = 3
        versionName = "${rootProject.version}-flight"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables {
            useSupportLibrary = true
        }
    }

    signingConfigs {
        val signingProps = Properties().apply {
            val file = rootProject.file("signing.properties")
            if (file.exists()) {
                file.inputStream().use { load(it) }
            }
        }
        create("release") {
            storeFile = signingProps["RELEASE_STORE_FILE"]?.let { rootProject.file(it as String) }
            storePassword = signingProps["RELEASE_STORE_PASSWORD"] as String?
            keyAlias = signingProps["RELEASE_KEY_ALIAS"] as String?
            keyPassword = signingProps["RELEASE_KEY_PASSWORD"] as String?
            enableV1Signing = true
            enableV2Signing = true
        }
    }

    buildTypes {
        debug {
            signingConfig = signingConfigs.getByName("release")
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
            signingConfig = signingConfigs.getByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }

    buildFeatures {
        compose = true
    }

    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

compose {
    desktop {
        application {
            mainClass = "com.mocharealm.accompanist.sample.MainKt"

            nativeDistributions {
                targetFormats(TargetFormat.Dmg, TargetFormat.Exe)
            }
        }
    }
    resources {
        packageOfResClass = "com.mocharealm.accompanist.sample"
        publicResClass = true
        generateResClass = always
        customDirectory(
            sourceSetName = "commonMain",
            directoryProvider = provider {
                layout.projectDirectory.dir("src/commonMain/resources")
            }
        )
    }
}