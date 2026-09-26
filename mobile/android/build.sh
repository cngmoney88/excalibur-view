#!/usr/bin/env bash
# Builds Excalibur View for Android: the Rust library for each processor, the
# PDF engine beside it, then a debug APK to install by hand and a release App
# Bundle for Play.
#
#   mobile/android/build.sh
#
# What it needs:
#   - Rust, with the Android targets:
#       rustup target add aarch64-linux-android x86_64-linux-android
#   - The Android NDK, r28 or later, at ANDROID_NDK_HOME (or ANDROID_NDK_ROOT,
#     or the newest one under $ANDROID_HOME/ndk).
#   - The Android SDK at ANDROID_HOME, with "platforms;android-36" and
#     "build-tools;36.1.0". Gradle finds it there.
#   - JDK 17 or later.
#
# Settings, all optional:
#   ABIS            which processors, default "arm64-v8a x86_64". arm64-v8a is
#                   every phone and tablet; x86_64 is Chromebooks and the
#                   emulator.
#   PDFIUM_BUILD    the pdfium-binaries build to pack, default below.
#   STEPS           "native gradle" by default; "native" stops before Gradle.
#   GRADLE          the Gradle to run, default the wrapper beside this file.
#
# Signing the release bundle: set EXV_KEYSTORE, EXV_KEYSTORE_PASSWORD,
# EXV_KEY_ALIAS and EXV_KEY_PASSWORD (or the exv.* Gradle properties; see
# app/build.gradle.kts). Without them the bundle is built unsigned. Keys are
# never committed.
#
# What it leaves behind, in dist/android at the top of the repository:
#   excalibur-view-<version>-debug.apk     install with `adb install`
#   excalibur-view-<version>.aab           upload to Play
#   native-debug-symbols-<version>.zip     upload beside the bundle, so Play's
#                                          crash reports have function names

set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"

abis="${ABIS:-arm64-v8a x86_64}"
steps="${STEPS:-native gradle}"
# The newest pdfium-binaries build when this was written. Its exports are a
# superset of the 7881 build pdfium-render 0.9's bindings are written against
# and the desktop carries (third_party/pdfium/embedded/VERSION).
pdfium_build="${PDFIUM_BUILD:-8066}"
# The oldest Android the app runs on, as in app/build.gradle.kts.
min_api=26

version="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' "$root/Cargo.toml")"
target_dir="${CARGO_TARGET_DIR:-$root/target}"
jnilibs="$here/app/src/main/jniLibs"
work="$here/build"
symbols="$work/native-symbols"
dist="$root/dist/android"

say() { printf '\n== %s\n' "$*"; }
fail() { printf 'build.sh: %s\n' "$*" >&2; exit 1; }

find_ndk() {
    for candidate in "${ANDROID_NDK_HOME:-}" "${ANDROID_NDK_ROOT:-}"; do
        if [ -n "$candidate" ] && [ -d "$candidate/toolchains/llvm" ]; then
            echo "$candidate"
            return
        fi
    done
    if [ -n "${ANDROID_HOME:-}" ] && [ -d "$ANDROID_HOME/ndk" ]; then
        local newest
        newest="$(ls -1 "$ANDROID_HOME/ndk" | sort -V | tail -1)"
        if [ -n "$newest" ]; then
            echo "$ANDROID_HOME/ndk/$newest"
            return
        fi
    fi
    fail "no Android NDK: set ANDROID_NDK_HOME"
}

# The NDK's own name for the machine it runs on. Macs have one universal
# toolchain under the Intel name.
host_tag() {
    case "$(uname -s)" in
        Linux) echo linux-x86_64 ;;
        Darwin) echo darwin-x86_64 ;;
        *) fail "build on Linux or macOS" ;;
    esac
}

# Fails unless every loadable segment of a library is aligned to 16 KB. Phones
# with 16 KB memory pages will not load anything less, and Play refuses bundles
# that carry it.
check_alignment() {
    local library="$1" readelf="$2"
    local aligns
    aligns="$("$readelf" -lW "$library" | awk '$1 == "LOAD" { print $NF }' | sort -u | tr '\n' ' ')"
    for align in $aligns; do
        if [ $((align)) -lt 16384 ]; then
            fail "$library has a segment aligned to $align, not 16 KB"
        fi
    done
    echo "  $(basename "$library"): LOAD aligned $aligns"
}

build_native() {
    local ndk tools
    ndk="$(find_ndk)"
    tools="$ndk/toolchains/llvm/prebuilt/$(host_tag)/bin"
    [ -x "$tools/llvm-strip" ] || fail "$ndk does not look like an NDK"
    say "NDK $(sed -n 's/^Pkg.Revision *= *//p' "$ndk/source.properties")"

    rm -rf "$jnilibs" "$symbols"
    mkdir -p "$work/pdfium"

    for abi in $abis; do
        local triple pdfium_arch
        case "$abi" in
            arm64-v8a) triple=aarch64-linux-android; pdfium_arch=arm64 ;;
            x86_64) triple=x86_64-linux-android; pdfium_arch=x64 ;;
            *) fail "no recipe for $abi" ;;
        esac
        local upper
        upper="$(echo "$triple" | tr 'a-z-' 'A-Z_')"
        local under="${triple//-/_}"

        say "Excalibur View $version for $abi"
        # The C and C++ in the dependency tree (GameActivity's glue among it)
        # are compiled by the NDK's clang for the oldest Android supported, and
        # the result is linked with 16 KB pages.
        export "CC_$under=$tools/$triple$min_api-clang"
        export "CXX_$under=$tools/$triple$min_api-clang++"
        export "AR_$under=$tools/llvm-ar"
        export "CARGO_TARGET_${upper}_LINKER=$tools/$triple$min_api-clang"
        export "CARGO_TARGET_${upper}_RUSTFLAGS=-C link-arg=-Wl,-z,max-page-size=16384"
        cargo build --manifest-path "$root/Cargo.toml" --locked --release \
            -p excalibur-view-mobile --target "$triple"

        local built="$target_dir/$triple/release/libexcalibur_view_mobile.so"
        [ -f "$built" ] || fail "cargo finished but $built is not there"
        mkdir -p "$jnilibs/$abi" "$symbols/$abi"
        # The unstripped copy is kept for Play's crash reports; the app gets
        # the stripped one.
        cp "$built" "$symbols/$abi/"
        "$tools/llvm-strip" --strip-unneeded -o "$jnilibs/$abi/libexcalibur_view_mobile.so" "$built"

        say "PDF engine, chromium/$pdfium_build, for $abi"
        local archive="$work/pdfium/$pdfium_build-android-$pdfium_arch.tgz"
        if [ ! -s "$archive" ]; then
            curl -fsSL --retry 5 --retry-all-errors --retry-delay 5 \
                "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F$pdfium_build/pdfium-android-$pdfium_arch.tgz" \
                -o "$archive.part"
            mv "$archive.part" "$archive"
        fi
        local unpacked="$work/pdfium/$pdfium_build-$pdfium_arch"
        rm -rf "$unpacked"
        mkdir -p "$unpacked"
        tar -xzf "$archive" -C "$unpacked"
        [ -f "$unpacked/lib/libpdfium.so" ] || fail "no lib/libpdfium.so in $archive"
        cp "$unpacked/lib/libpdfium.so" "$jnilibs/$abi/"

        for library in "$jnilibs/$abi"/*.so; do
            check_alignment "$library" "$tools/llvm-readelf"
        done
        ls -l "$jnilibs/$abi"
    done

    mkdir -p "$dist"
    rm -f "$dist/native-debug-symbols-$version.zip"
    (cd "$symbols" && zip -qr "$dist/native-debug-symbols-$version.zip" .)
}

build_gradle() {
    local gradle="${GRADLE:-$here/gradlew}"
    say "Gradle: debug APK and release bundle"
    (cd "$here" && "$gradle" --no-daemon --stacktrace assembleDebug bundleRelease)

    mkdir -p "$dist"
    cp "$here/app/build/outputs/apk/debug/app-debug.apk" "$dist/excalibur-view-$version-debug.apk"
    cp "$here/app/build/outputs/bundle/release/app-release.aab" "$dist/excalibur-view-$version.aab"
    say "Done"
    ls -l "$dist"
}

for step in $steps; do
    case "$step" in
        native) build_native ;;
        gradle) build_gradle ;;
        *) fail "no step called $step" ;;
    esac
done
