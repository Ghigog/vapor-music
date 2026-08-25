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
        // R8 is neutered from `app/proguard-rules.pro`, not from here.
        //
        // Two attempts to set `isMinifyEnabled = false` from this file both
        // failed, and both failed silently:
        //
        //   1. In the `configure` block below. `plugins.withId` fires the
        //      moment the Android plugin is applied — the first line of
        //      `app/build.gradle.kts` — so the template's own release block ran
        //      afterwards and set it back. v2.0.0-rc.4 shipped that way.
        //   2. From `afterEvaluate`. AGP registers its own `afterEvaluate` when
        //      the plugin is applied, which is *before* this one, and it has
        //      created the variants by the time this runs. v2.0.0-rc.5 failed
        //      the APK check on it — caught this time, published nothing.
        //
        // There is no third guess at the lifecycle. `-dontshrink`,
        // `-dontoptimize` and `-dontobfuscate` in `app/proguard-rules.pro` make
        // R8 a pass-through instead, which is the same outcome by a route that
        // does not depend on when anything is evaluated: that file is ours, the
        // CLI does not rewrite it (AND-3 covers `build.gradle.kts` only), and
        // the template already wires it in through `proguardFiles`.

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

