import com.android.build.api.dsl.androidLibrary
import com.vanniktech.maven.publish.JavadocJar
import com.vanniktech.maven.publish.KotlinMultiplatform

plugins {
    alias(libs.plugins.jetbrains.kotlin.multiplatform)
    alias(libs.plugins.jetbrains.compose)
    alias(libs.plugins.android.kotlin.multiplatform.library)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.maven.publish)
    alias(libs.plugins.dokka)
}

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
        compileSdk = 37

        minSdk = 21

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
    val configFile = rootProject.layout.projectDirectory.file("compose-compiler-config.conf")
    if (configFile.asFile.exists()) {
        stabilityConfigurationFiles.add(configFile)
    }
}