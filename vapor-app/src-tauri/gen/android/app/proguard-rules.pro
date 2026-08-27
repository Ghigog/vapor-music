# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile
# ---------------------------------------------------------------------------
# Added 2026-08-24, for the first release APK there has ever been.
#
# Every Android build before this one was `--debug`, where R8 does not run. The
# release build type has `isMinifyEnabled = true`, so the first signed APK is
# also the first time anything in this project has been shrunk and renamed —
# and the classes most likely to be renamed out from under a caller are exactly
# the ones no Kotlin code calls.
#
# `MainActivity` and `PlaybackService` are reached from Rust over JNI, by
# string. R8 keeps a class named in `AndroidManifest.xml` but is free to rename
# and drop its methods, which is the failure that looks like AND-4 all over
# again: the app starts, renders, and has no sound. `vapor_dsp` is not the
# thing that would break — a JNI lookup that returns null is.
#
# Kept whole rather than method by method. The Kotlin half of this app is two
# files; the APK is dominated by `libvapor_app_lib.so`, which R8 never touches,
# so what this gives up is not measurable in a download.
-keep class com.dylangrowcoot.vapormusic.** { *; }

# Native method names, wherever they are. `-keepclasseswithmembernames` keeps
# the class and the member and renames neither, which is the whole requirement
# for a symbol resolved by string at runtime.
-keepclasseswithmembernames class * {
    native <methods>;
}

# ---------------------------------------------------------------------------
# R8 does nothing. Added 2026-08-25, after two failed attempts to turn it off.
#
# `isMinifyEnabled = true` is in Tauri's own template for the release build
# type, and the CLI rewrites `app/build.gradle.kts` from that template on every
# `tauri android build` (AND-3). Overriding the property from the root build
# script does not work: set in a `plugins.withId` callback it is overwritten by
# the template moments later (v2.0.0-rc.4 shipped minified because of it), and
# set in `afterEvaluate` it lands after AGP has already created the variants
# (v2.0.0-rc.5 failed the APK check on it).
#
# These three do not depend on Gradle's evaluation order at all. R8 still runs
# as a dexer and changes nothing on the way through, which is the outcome that
# was wanted. This file is not regenerated, and the template already passes it
# to `proguardFiles`.
#
# Why R8 is unwanted: v2.0.0-rc.3 was the first release APK this project ever
# built and the first time R8 ever ran here, and it crashed on launch. The JNI
# symbols, the classes wry resolves by string, the embedded frontend and the
# native libraries were all verified intact in that APK, so the keep rules
# above were not the problem.
#
# CORRECTED 2026-08-27: R8 was not the problem either. These three rules did
# work — rc.7's APK is provably unobfuscated — and it crashed exactly the same,
# because the crash was `PlaybackService` breaking the `startForegroundService`
# / `startForeground` contract and being killed for it. See AND-5.
#
# They stay because R8 buys this project nothing (the APK is dominated by
# `libvapor_app_lib.so`, which R8 never touches) and turning it back on is a
# change that wants a device to hand. They are not a fix for anything.
#
# `scripts/verify-apk.mjs` fails the build if any obfuscated class name reaches
# the APK, so if these stop working it is red rather than quiet. Re-enabling
# minification means deleting these rules *and* that check, deliberately,
# with a device to hand.
-dontshrink
-dontoptimize
-dontobfuscate
-dontwarn **
-ignorewarnings

