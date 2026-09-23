#!/usr/bin/env python3
"""Publishing a release, for somebody who should not have to remember how.

Double-click "Release Excalibur View" on the PC and this runs. It finds the
signing key, the GitHub token, the Mac files and the release notes by looking
where they actually live, shows you what it found, and waits for you to say go.

Then it does what `publish.py release` does — tests, build, sign, upload,
publish — and says in plain words what happened.

The signing key is read here and used here. That is the reason this runs on
this computer rather than somewhere more convenient.
"""

import io
import json
import os
import subprocess
import sys
import urllib.error
import urllib.request
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

# Where things have actually been kept, in the order worth looking.
KEY_NAMES = ("hyperview-signing.key",)
TOKEN_NAMES = ("github-token.txt", "github-token.txt.txt")
REPO = "cngmoney88/hyperview-releases"
# Where the Mac build happens, so this does not have to happen on a Mac.
SOURCE_REPO = "cngmoney88/excalibur"


def bold(text):
    return f"\033[1m{text}\033[0m" if sys.stdout.isatty() else text


def say(text=""):
    print(text, flush=True)


def stop(why, *extra):
    say()
    say(bold(why))
    for line in extra:
        say("  " + line)
    say()
    say("Nothing was published.")
    wait_to_close()
    raise SystemExit(1)


def wait_to_close():
    say()
    try:
        input("Press return to close this window. ")
    except EOFError:
        pass


def places():
    """Folders worth looking in for the key, the token and the Mac files."""
    home = os.path.expanduser("~")
    found = [ROOT, os.path.dirname(ROOT)]
    found += [os.path.join(os.path.dirname(ROOT), name)
              for name in ("ExcaliburSigning", "ExcaliburView")]
    found += [os.path.join(home, name) for name in ("Desktop", "Downloads", "OneDrive")]
    onedrive = os.path.join(home, "OneDrive")
    if os.path.isdir(onedrive):
        found.append(os.path.join(onedrive, "Desktop"))
    return [p for p in found if os.path.isdir(p)]


def find_file(names, extra_dirs=()):
    for folder in list(extra_dirs) + places():
        for name in names:
            path = os.path.join(folder, name)
            if os.path.isfile(path):
                return path
    return None


def find_by_suffix(suffix, contains=""):
    """The newest file ending in `suffix`, searched one level deep so a folder
    dropped on the Desktop by the Mac is found without being unpacked."""
    hits = []
    for folder in places():
        try:
            entries = os.listdir(folder)
        except OSError:
            continue
        for entry in entries:
            path = os.path.join(folder, entry)
            if os.path.isfile(path) and entry.endswith(suffix) and contains in entry:
                hits.append(path)
            elif os.path.isdir(path) and "Excalibur" in entry:
                try:
                    for inner in os.listdir(path):
                        deeper = os.path.join(path, inner)
                        if os.path.isfile(deeper) and inner.endswith(suffix) and contains in inner:
                            hits.append(deeper)
                except OSError:
                    pass
    hits.sort(key=lambda p: os.path.getmtime(p), reverse=True)
    return hits[0] if hits else None


def github(url, token, accept="application/vnd.github+json"):
    request = urllib.request.Request(url, headers={
        "Accept": accept,
        "Authorization": f"Bearer {token}",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "excalibur-release",
    })
    return urllib.request.urlopen(request, timeout=120)


def mac_build_from_github(version, token):
    """Fetches the Mac build GitHub made for this version.

    A Mac is the only machine that can sign and notarize a Mac program, so
    GitHub keeps one for exactly that. What comes back is the same two files a
    Mac would have produced, and they are checked against the signing key here
    like anything else.

    Returns (zip, dmg) as paths, or (None, None) with a reason printed.
    """
    want = f"mac-{version}"
    try:
        url = f"https://api.github.com/repos/{SOURCE_REPO}/actions/artifacts?per_page=100"
        with github(url, token) as answer:
            artifacts = json.load(answer).get("artifacts", [])
    except urllib.error.HTTPError as e:
        say(f"  GitHub said {e.code} when asked for the Mac build.")
        return None, None
    except Exception as e:  # a laptop on a hotel network, most likely
        say(f"  Could not reach GitHub: {e}")
        return None, None

    mine = [a for a in artifacts if a.get("name") == want and not a.get("expired")]
    if not mine:
        names = sorted({a.get("name", "") for a in artifacts if str(a.get("name", "")).startswith("mac-")})
        say(f"  GitHub has no Mac build for {version}.")
        if names:
            say(f"  It has: {', '.join(names[:6])}")
        return None, None

    newest = max(mine, key=lambda a: a.get("created_at", ""))
    into = os.path.join(ROOT, "target", "mac-from-github")
    os.makedirs(into, exist_ok=True)
    say(f"  Downloading the Mac build GitHub made on {newest.get('created_at', '')[:10]}...")
    try:
        with github(newest["archive_download_url"], token, accept="application/zip") as answer:
            blob = answer.read()
    except Exception as e:
        say(f"  The download did not finish: {e}")
        return None, None

    with zipfile.ZipFile(io.BytesIO(blob)) as bundle:
        bundle.extractall(into)
    zip_path = dmg_path = None
    for name in os.listdir(into):
        full = os.path.join(into, name)
        if name.endswith("ExcaliburView-mac.zip"):
            zip_path = full
        elif name.endswith(".dmg"):
            dmg_path = full
    return zip_path, dmg_path


def version_in_cargo():
    with open(os.path.join(ROOT, "Cargo.toml"), encoding="utf-8") as f:
        for line in f:
            if line.startswith("version"):
                return line.split('"')[1]
    stop("Could not read the version out of Cargo.toml.")


def shorten(path, keep=3):
    if not path:
        return "not found"
    parts = os.path.abspath(path).replace("\\", "/").split("/")
    return os.sep.join(parts[-keep:]) if len(parts) > keep else path


def run(args):
    return subprocess.call(args, cwd=ROOT)


def main():
    promote = "--promote" in sys.argv[1:]

    say()
    say(bold("   Excalibur View — publishing a release"))
    say()

    # The code first. Publishing something older than what is on GitHub is a
    # mistake that is invisible until somebody reports a fixed bug.
    say(bold("Getting the latest code"))
    if subprocess.call(["git", "pull", "--ff-only"], cwd=ROOT) != 0:
        stop("Could not get the latest code.",
             "If you have local edits here, commit them or put them aside first.")

    version = version_in_cargo()
    key = find_file(KEY_NAMES)
    token = find_file(TOKEN_NAMES)
    notes = find_file((f"notes-{version}.txt", "notes.txt"), extra_dirs=[ROOT])
    if not key:
        stop("The signing key is not where this expects it.",
             "It is looked for by name, hyperview-signing.key, beside this",
             "folder or in an ExcaliburSigning folder next to it — so it",
             "follows you to another computer without anything being edited.",
             "Without it nothing can be signed, and an unsigned build is one",
             "no copy of Excalibur View anywhere will install.")
    if not token:
        stop("The GitHub token is not where this expects it.",
             "It should be github-token.txt, in dev\\ExcaliburSigning.")
    say()
    say(bold("Getting the Mac build"))
    mac_zip, mac_dmg = (None, None)
    if token:
        mac_zip, mac_dmg = mac_build_from_github(version, open(token).read().strip())
    # Failing that, anything a Mac left lying about on this computer.
    if not mac_zip:
        mac_zip = find_by_suffix("ExcaliburView-mac.zip")
        mac_dmg = find_by_suffix(".dmg", contains=version)
        if mac_zip:
            say("  Using the Mac files found on this computer instead.")

    say()
    say(bold(f"Excalibur View {version}"))
    say()
    say(f"  signing key    {shorten(key)}")
    say(f"  GitHub token   {shorten(token)}")
    say(f"  release notes  {shorten(notes)}")
    say(f"  Mac update     {shorten(mac_zip)}")
    say(f"  Mac download   {shorten(mac_dmg)}")
    say()

    if promote:
        say(bold("This hands " + version + " to every office, not just the early ones."))
        say("Only do this once you have run it on a seat and it was fine.")
        if input("\nType yes to go ahead: ").strip().lower() not in ("y", "yes"):
            say("\nLeft as it was.")
            wait_to_close()
            return
        code = run([sys.executable, os.path.join("tools", "publish.py"), "promote",
                    "--key", key, "--token-file", token, "--repo", REPO,
                    "--version", version])
        if code != 0:
            stop("That did not go through. The message above says why.")
        say()
        say(bold(f"{version} is now the version every office takes."))
        wait_to_close()
        return

    if not mac_zip or not mac_dmg:
        say()
        say(bold("No Mac build found, so this would be a Windows-only release."))
        say("  That is a perfectly good release and every one before 0.6.4 was one.")
        say("  If you meant to include the Mac: on GitHub, open the Mac build")
        say("  workflow and press Run workflow. It takes about twenty minutes,")
        say("  most of it Apple. Then run this again.")
        say()
        if input("Publish Windows only? Type yes, or anything else to stop: ").strip().lower() not in ("y", "yes"):
            say("\nStopped. Nothing was published.")
            wait_to_close()
            return
        mac_zip = mac_dmg = None

    say("This will run the tests, build both Windows programs, sign everything")
    say("with your key, and publish it as an early release. Offices set to try")
    say("new versions first will take it; everybody else waits until you say so.")
    say()
    if input("Press return to go ahead, or type no to stop: ").strip().lower() in ("n", "no"):
        say("\nStopped. Nothing was published.")
        wait_to_close()
        return

    args = [sys.executable, os.path.join("tools", "publish.py"), "release",
            "--key", key, "--token-file", token, "--repo", REPO,
            "--version", version]
    if notes:
        args += ["--notes-file", notes]
    if mac_zip:
        args += ["--mac-zip", mac_zip, "--mac-dmg", mac_dmg]

    if run(args) != 0:
        stop("The release stopped before publishing.",
             "Whatever went wrong is in the message above. Running this again",
             "is safe — it finishes what it started rather than starting over.")

    say()
    say(bold("Published."))
    say()
    say(f"  https://github.com/{REPO}/releases/tag/v{version}")
    say()
    say("  The website's download page will show it within a few minutes.")
    say("  Offices on the early channel will be offered it within half an hour.")
    say()
    say("  When you have run it on one seat and it was fine, double-click")
    say("  'Give it to everyone' to hand it to the rest.")
    wait_to_close()


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        say("\n\nStopped. Nothing was published.")
