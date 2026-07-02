import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.jetbrains.compose)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.stability.analyzer)
}

fun getSecretProperty(key: String): String? {
    return System.getenv(key)
        ?: project.findProperty(key) as? String
}

android {
    namespace = "com.mocharealm.accompanist.sample"
    compileSdk = 37

    defaultConfig {
        applicationId = "com.mocharealm.accompanist.demo"
        minSdk = 29
        targetSdk = 37
        versionCode = 3
        versionName = "${rootProject.version}-flight"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables {
            useSupportLibrary = true
        }
    }

    signingConfigs {
        val sFile = getSecretProperty("RELEASE_STORE_FILE")
        val sPassword = getSecretProperty("RELEASE_STORE_PASSWORD")
        val kAlias = getSecretProperty("RELEASE_KEY_ALIAS")
        val kPassword = getSecretProperty("RELEASE_KEY_PASSWORD")

        if (sFile != null && sPassword != null && kAlias != null && kPassword != null) {
            create("release") {
                val keystoreFile = rootProject.file(sFile)
                if (keystoreFile.exists()) {
                    storeFile = keystoreFile
                    storePassword = sPassword
                    keyAlias = kAlias
                    keyPassword = kPassword
                } else {
                    logger.warn("Keystore file not found at: ${keystoreFile.absolutePath}")
                }
                enableV1Signing = true
                enableV2Signing = true
                enableV3Signing = true
                enableV4Signing = true
            }
        }
    }

    buildTypes {
        debug {
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
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
        jniLibs {
            useLegacyPackaging = true
        }
    }

    splits {
        abi {
            isEnable = true
            reset()
            include("arm64-v8a")
            include("armeabi-v7a")
            isUniversalApk = false
        }
    }
}

kotlin {
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
    implementation(compose.components.uiToolingPreview)

    implementation(libs.accompanist.lyrics.core)
    implementation(libs.gaze.capsule)

    implementation(platform(libs.koin.bom))
    implementation(libs.koin.compose)
    implementation(libs.koin.compose.viewmodel)

    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.media3.exoplayer)
    implementation(libs.androidx.media3.session)
    implementation(files("src/main/libs/lib-decoder-ffmpeg-release.aar"))
    implementation(libs.cloudy)
    implementation(libs.kotlinx.coroutines.guava)
}

composeCompiler {
    stabilityConfigurationFiles.add(rootProject.layout.projectDirectory.file("compose_compiler_config.conf"))
}
