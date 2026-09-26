pluginManagement {
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
            }
        }
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        maven {
            url = uri("https://maven.aliyun.com/repository/public")
            mavenContent {
                includeGroupAndSubgroups("com.github.promeg")
            }
        }
        maven {
            url = uri("file:///E:/maven")
            mavenContent {
                includeGroupAndSubgroups("com.mocharealm")
            }
        }
        google()
        mavenCentral()
    }
}

rootProject.name = "lyrics-ui"
include(":src")
include(":sample:shared")
include(":sample:android-app")
include(":sample:desktop-app")
 