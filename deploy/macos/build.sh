#!/usr/bin/env bash
#
# Builds, signs and notarizes Excalibur View for macOS.
#
# Run it on the Mac, from the root of the repository:
#
#     deploy/macos/build.sh
#
# It makes a universal binary (Apple Silicon and Intel in one file, so there
# is one download for every Mac), wraps it in an .app, signs it with your
# Developer ID, sends it to Apple to be notarized, and staples the result so
# it opens with no warning even on a machine that is offline.
#
# What it needs, once:
#   - Xcode command line tools:  xcode-select --install
#   - Rust, and both targets:
#       rustup target add aarch64-apple-darwin x86_64-apple-darwin
#   - A "Developer ID Application" certificate in your login keychain.
#     Team ID: C27WQYHJXB
#   - A notary profile stored in the keychain, so no secret is ever typed
#     into a script or a terminal history:
#       xcrun notarytool store-credentials excalibur-notary \
#         --key ~/private/AuthKey_XXXXXXXX.p8 \
#         --key-id XXXXXXXX --issuer XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX
#
# Set these two, or pass them in the environment:
: "${SIGN_IDENTITY:=Developer ID Application: Creede Guardamondo (C27WQYHJXB)}"
: "${NOTARY_PROFILE:=excalibur-notary}"

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

name="Excalibur View"
bundle_id="com.excaliburct.view"
version="$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')"
out="$root/target/macos"
app="$out/$name.app"

say() { printf '\n== %s\n' "$*"; }

say "Excalibur View $version"

# ---- 1. build both architectures ------------------------------------------
say "Building for Apple Silicon and Intel"
cargo build --release --target aarch64-apple-darwin -p hyperview
cargo build --release --target x86_64-apple-darwin  -p hyperview

# ---- 2. one binary that runs on both ---------------------------------------
say "Joining them into one universal binary"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/$name" \
  "target/aarch64-apple-darwin/release/hyperview" \
  "target/x86_64-apple-darwin/release/hyperview"
chmod +x "$app/Contents/MacOS/$name"
lipo -info "$app/Contents/MacOS/$name"

# ---- 3. the bundle ----------------------------------------------------------
say "Making the app bundle"
sed "s/__VERSION__/$version/g" deploy/macos/Info.plist > "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"
if [ -f deploy/macos/AppIcon.icns ]; then
  cp deploy/macos/AppIcon.icns "$app/Contents/Resources/AppIcon.icns"
else
  echo "   no deploy/macos/AppIcon.icns — the app will use the generic icon"
fi

# ---- 4. sign ----------------------------------------------------------------
say "Signing"
# Anything nested is signed before the bundle that holds it, innermost first.
find "$app/Contents" -type f \( -name '*.dylib' -o -name '*.so' \) -print0 |
  while IFS= read -r -d '' lib; do
    codesign --force --timestamp --options runtime --sign "$SIGN_IDENTITY" "$lib"
  done
codesign --force --timestamp --options runtime \
  --entitlements deploy/macos/entitlements.plist \
  --sign "$SIGN_IDENTITY" "$app"
codesign --verify --deep --strict --verbose=2 "$app"

# ---- 5. a disk image to hand people -----------------------------------------
say "Making the disk image"
dmg="$out/ExcaliburView-$version.dmg"
rm -f "$dmg"
staging="$out/dmg"
rm -rf "$staging"; mkdir -p "$staging"
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Applications"
hdiutil create -volname "$name" -srcfolder "$staging" -ov -format UDZO "$dmg" >/dev/null
rm -rf "$staging"
codesign --force --timestamp --sign "$SIGN_IDENTITY" "$dmg"

# ---- 6. notarize ------------------------------------------------------------
say "Sending it to Apple to be notarized (this takes a few minutes)"
xcrun notarytool submit "$dmg" --keychain-profile "$NOTARY_PROFILE" --wait

say "Stapling, so it opens even on a Mac that is offline"
xcrun stapler staple "$dmg"
xcrun stapler validate "$dmg"

# ---- 7. what Gatekeeper will say --------------------------------------------
say "What Gatekeeper says about it"
spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg" || true

say "Done"
echo "   $dmg"
echo
echo "Check it opens on a Mac that has never seen it:"
echo "   xattr -w com.apple.quarantine '0081;00000000;Safari;' \"$dmg\" && open \"$dmg\""
