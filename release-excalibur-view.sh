#!/usr/bin/env bash
#
# The same thing as "Release Excalibur View.bat", for a Mac or a Linux box.
#
# Whichever machine ends up holding hyperview-signing.key is the machine that
# runs this. Nothing about it assumes Windows: the script it calls finds the
# key, the token and the notes by name rather than by a path somebody typed in
# once.

cd "$(dirname "${BASH_SOURCE[0]}")" || exit 1
exec python3 tools/release_now.py "$@"
