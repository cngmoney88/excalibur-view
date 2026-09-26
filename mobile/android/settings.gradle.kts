// Excalibur View for Android. The program itself is Rust, built by build.sh
// into app/src/main/jniLibs; this project only packs it.

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "ExcaliburView"
include(":app")
