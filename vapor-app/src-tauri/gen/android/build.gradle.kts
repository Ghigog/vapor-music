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
        // R8 off, and it has to be set from `afterEvaluate` to stay off.
        //
        // The first attempt at this set `isMinifyEnabled = false` in the
        // `configure` block above, and it silently did nothing. `plugins.withId`
        // fires the moment the Android plugin is applied — which is the first
        // line of `app/build.gradle.kts` — so the assignment ran, and then that
        // file's own `buildTypes { getByName("release") { isMinifyEnabled =
        // true } }` executed afterwards and put it back.
        //
        // Nothing reported this. The build was green, the APK was byte-for-byte
        // the same size as the one before it, and v2.0.0-rc.4 shipped with R8
        // still on. `applicationIdSuffix` above survives only because no other
        // build script sets that property; anything the template *does* set has
        // to be overridden after the template has had its turn.
        //
        // Why R8 is off at all: rc.3 was the first release APK this project
        // ever produced, and the first time R8 ran against it. It crashed on
        // launch. The JNI names, the classes wry resolves by string, the
        // embedded frontend and the native libraries were all verified intact
        // in that APK, so the keep rules in `app/proguard-rules.pro` were not
        // the problem — the likeliest remaining mechanism is field renaming on
        // the plugin config classes, which are deserialised reflectively at
        // startup. Unconfirmed, and not worth another guess.
        //
        // `scripts/verify-apk.mjs` now fails the build if R8 ran at all, so
        // this cannot go quiet a second time. Turning minification back on is a
        // task with a phone in someone's hand, and it means changing that check
        // too — deliberately, which is the point.
        afterEvaluate {
            extensions.configure<com.android.build.gradle.AppExtension>("android") {
                buildTypes.getByName("release").isMinifyEnabled = false
            }
        }

        extensions.configure<com.android.build.gradle.AppExtension>("android") {
            buildTypes.getByName("debug").applicationIdSuffix = ".debug"

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

