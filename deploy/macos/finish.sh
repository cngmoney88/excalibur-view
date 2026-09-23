#!/usr/bin/env bash
#
# Makes the update file out of a build Apple has already notarized.
#
#     deploy/macos/finish.sh
#
# Use this when `build.sh` notarized and stapled the disk image but stopped
# before making the zip -- or any time you have a notarized build and want the
# zip without waiting on Apple a second time. Apple's ticket already exists for
# this program; stapling only writes it into the file, which takes no time and
# no network.
#
# It does not build, sign or notarize anything. If the program in target/macos
# is not the one that was notarized, stapling fails and says so.

: "${NOTARY_PROFILE:=excalibur-notary}"

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

name="Excalibur View"
out="$root/target/macos"
app="$out/$name.app"
zip="$out/ExcaliburView-mac.zip"

say() { printf '\n== %s\n' "$*"; }

if [ ! -d "$app" ]; then
  echo "There is no $app to finish. Run deploy/macos/build.sh first."
  exit 1
fi

say "Writing Apple's ticket into the program"
xcrun stapler staple "$app"
xcrun stapler validate "$app"

say "Zipping it, which is how an update arrives"
rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"

say "Checking the zip unpacks into a program that still opens"
check="$out/check"
rm -rf "$check"; mkdir -p "$check"
ditto -x -k "$zip" "$check"
codesign --verify --strict --deep --verbose=2 "$check/$name.app"
xcrun stapler validate "$check/$name.app"
spctl --assess --type exec --verbose=2 "$check/$name.app"
rm -rf "$check"

say "Done"
echo "   $zip"
echo
echo "Both this and the .dmg go on the release. The manifest names the zip,"
echo "because the manifest is read by the updater; the disk image is what the"
echo "website offers a person."
