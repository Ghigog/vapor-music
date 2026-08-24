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
