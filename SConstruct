import os
import sys

# Load SCons variables from arguments
opts = Variables([], ARGUMENTS)

# Configure platform and target parameters
opts.Add(EnumVariable("platform", "Target platform", sys.platform, allowed_values=["linux", "osx", "windows", "android", "ios", "macos"]))
opts.Add(EnumVariable("target", "Compilation target", "template_debug", allowed_values=["template_debug", "template_release"]))
opts.Add(EnumVariable("arch", "Target architecture", "universal", allowed_values=["x86_32", "x86_64", "arm32", "arm64", "universal"]))

env = Environment(variables=opts)
Help(opts.GenerateHelpText(env))

# Normalize platform name for macOS
if env["platform"] == "osx":
    env["platform"] = "macos"

# Set architecture defaults for Mac if universal is not supported by scons directly
if env["platform"] == "macos" and env["arch"] == "universal":
    # Default to current host architecture to simplify local compilation
    import platform
    env["arch"] = "arm64" if platform.machine() == "arm64" else "x86_64"

# Export env for godot-cpp to import
Export("env")

# Import godot-cpp SConstruct (which builds the bindings library)
SConscript("godot-cpp/SConstruct")

# Append custom include and library paths (Local build directories first, then Homebrew default paths on Mac)
env.Append(CPPPATH=["src", "external/build/include", "/opt/homebrew/include", "/opt/homebrew/include/eigen3", "/usr/local/include", "/usr/local/include/eigen3"])
env.Append(LIBPATH=["external/build/lib", "/opt/homebrew/lib", "/usr/local/lib"])
env.Append(LIBS=["essentia", "rubberband"])
env.Append(CXXFLAGS=["-fexceptions"])

# Configure macOS link flags and dynamic library naming
sources = Glob("src/*.cpp")

if env["platform"] == "macos":
    suffix = ".macos.debug.dylib" if env["target"] == "template_debug" else ".macos.release.dylib"
    # Ensure system dynamic linker searches local build and Homebrew directories for dependencies at runtime
    env.Append(LINKFLAGS=["-Wl,-rpath,@loader_path/../external/build/lib", "-Wl,-rpath,/opt/homebrew/lib", "-Wl,-rpath,/usr/local/lib"])
    # Media Player framework backs the Now Playing / hardware media key bridge.
    env.Append(LINKFLAGS=["-framework", "MediaPlayer", "-framework", "Foundation", "-framework", "AppKit"])
    env.Append(CXXFLAGS=["-fobjc-arc"])
    sources += Glob("src/platform/macos/*.mm")
elif env["platform"] == "windows":
    suffix = ".windows.debug.%s.dll" % env["arch"] if env["target"] == "template_debug" else ".windows.release.%s.dll" % env["arch"]
    # windowsapp.lib is the umbrella import lib for C++/WinRT projection + activation.
    env.Append(LIBS=["windowsapp"])
    env.Append(CXXFLAGS=["/std:c++17", "/EHsc"])
    sources += Glob("src/platform/windows/*.cpp")
else:
    # Fallback suffix for other systems (development stubs)
    suffix = env["SHLIBSUFFIX"]

library_name = "bin/libaudio_dsp" + suffix

library = env.SharedLibrary(target=library_name, source=sources)
Default(library)
