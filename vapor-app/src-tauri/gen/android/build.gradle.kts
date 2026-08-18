buildscript {
    repositories {
        google()
        mavenCentral()
    }
    dependencies {
        classpath("com.android.tools.build:gradle:8.11.0")
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:1.9.25")
    }
}

allprojects {
    repositories {
        google()
        mavenCentral()
    }
}

// A debug build installs alongside the real app rather than replacing it.
//
// This belongs in `app/build.gradle.kts`, and cannot live there: the Tauri CLI
// rewrites that file from its template on every `tauri android build`, so an
// edit to it survives exactly until the next build (AND-3). This file it leaves
// alone.
//
// Without the suffix both builds are `com.dylangrowcoot.vapormusic`. They are
// signed with different keys — the debug keystore against whatever signed the
// release on the device — so Android refuses the install outright, and the only
// way through is to uninstall the other one, taking its settings, its playlists
// and its stored password with it. A test build must not be able to cost
// someone their data.
subprojects {
    plugins.withId("com.android.application") {
        extensions.configure<com.android.build.gradle.AppExtension>("android") {
            buildTypes.getByName("debug").applicationIdSuffix = ".debug"
        }
    }
}

tasks.register("clean").configure {
    delete("build")
}

