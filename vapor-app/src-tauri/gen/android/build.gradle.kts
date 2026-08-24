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
//
// Release signing lives here for the same reason, and has one more:
// `keystore.properties` is read only if it exists, so a checkout without it —
// anyone's but Dylan's, and this file is public — configures no signing at all
// and a release build fails saying so. That is the honest failure. The
// alternative, a hardcoded fallback, is how a key nobody chose ends up on an
// APK somebody installed.
//
// The file is written by `.github/workflows/release.yml` from four repository
// secrets and deleted again in the same job. `.gitignore` in this directory
// has covered it since before it existed, which is the order docs/RELEASE.md
// asks for and the only order that is any use.
subprojects {
    plugins.withId("com.android.application") {
        extensions.configure<com.android.build.gradle.AppExtension>("android") {
            buildTypes.getByName("debug").applicationIdSuffix = ".debug"

            // R8 off, until something has actually proved it safe here.
            //
            // The v2.0.0-rc.3 APK crashed on launch. It is the first release
            // APK this project has ever produced, which makes it also the first
            // time R8 has ever run: every earlier Android build was `--debug`,
            // where minification is off, and the debug build launched fine on a
            // Pixel 9 (AND-4). Between the build that works and the build that
            // does not there are two differences — the signing key and R8 — and
            // a signature cannot crash a running process.
            //
            // `app/proguard-rules.pro` keeps `com.dylangrowcoot.vapormusic.**`
            // and every native method name, which is the rule that *should*
            // have covered it. It evidently did not cover enough — `MainActivity`
            // extends `TauriActivity`, and the Tauri and wry classes underneath
            // it are not in that package — and the honest response to "my keep
            // rules were incomplete" is not a second guess at the keep rules.
            //
            // What it costs: the Kotlin half of this app is two files. The APK
            // is dominated by `libvapor_app_lib.so`, which R8 never touches, so
            // the size difference is not something anybody downloading this
            // will notice.
            //
            // Turning it back on is a task with a device in someone's hand, not
            // a line changed on the way past. `app/build.gradle.kts` sets this
            // true in its release block; the CLI rewrites that file from its
            // template on every build (AND-3), so the override lives here.
            buildTypes.getByName("release").isMinifyEnabled = false

            val keystoreProperties = rootProject.file("keystore.properties")
            if (keystoreProperties.exists()) {
                val props = java.util.Properties()
                keystoreProperties.inputStream().use { props.load(it) }

                val release = signingConfigs.maybeCreate("release")
                release.storeFile = rootProject.file(props.getProperty("storeFile"))
                release.storePassword = props.getProperty("storePassword")
                release.keyAlias = props.getProperty("keyAlias")
                release.keyPassword = props.getProperty("keyPassword")

                buildTypes.getByName("release").signingConfig = release
            } else {
                logger.lifecycle(
                    "vapor: no gen/android/keystore.properties — release builds " +
                        "will be unsigned. See docs/RELEASE.md §1."
                )
            }
        }
    }
}

tasks.register("clean").configure {
    delete("build")
}

