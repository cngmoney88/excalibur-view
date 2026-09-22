#!/usr/bin/env python3
"""Publishing a Hyperview release: signing it, and putting it on GitHub.

Runs on the publisher's own computer, beside the signing key, because the key
never goes anywhere else. Standard library plus `cryptography` for Ed25519.

    python3 publish.py sign     --key hyperview-signing.key --version 0.5.0 \\
                                --channel preview --notes-file notes.txt \\
                                --app Hyperview.exe --server Hyperview-Server.exe \\
                                --out release.json
    python3 publish.py upload   --token-file github-token.txt --repo owner/name \\
                                --version 0.5.0 --notes-file notes.txt \\
                                release.json Hyperview.exe Hyperview-Server.exe
    python3 publish.py promote  --key hyperview-signing.key --token-file github-token.txt \\
                                --repo owner/name --version 0.5.0
    python3 publish.py sign-plugin --key hyperview-signing.key --id mesafab-estimating \\
                                --version 1.0.0 --name "Mesa Fab Estimating" \\
                                mesafab_estimating.wasm

A release goes up as `preview` first — a GitHub prerelease, which only an
office set to "get them early" takes — and is promoted to `stable` once it has
been tried. The channel is part of what is signed, so promoting re-signs the
manifest; the program files themselves do not change.

The signed text must match `hub::update::Release::signing_payload` byte for
byte. There is a test in the Rust code that checks a manifest this script made.

An Excalibur View Office license is signed with the same key too, into a
`.evlicense` file the customer adds under Studio -> Office -> License. Its
signed text starts with a different first line from a release's, so one can
never pass for the other, and must match `hub::license::License::signing_payload`:

    python3 publish.py sign-license --key hyperview-signing.key \\
                                --company "Acme Steel, Inc." --users 6 --years 1
    python3 publish.py sign-license --key hyperview-signing.key \\
                                --company "Mesa Fab, Inc." --founding
    python3 publish.py sign-license --key hyperview-signing.key --product citadel \\
                                --company "Arc Valley Construction" --founding

A plugin is signed with the same key, into a `.hvplugin` file that an office
administrator adds from Hyperview's Plugins menu. Its signed text must match
`plugin_api::signing_payload`, and a test in the Rust code checks a plugin
this script sealed.
"""

import argparse
import datetime
import hashlib
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request

APP_PLATFORM = "windows-x64"
SERVER_PLATFORM = "windows-x64-server"
# What each program is called on the release. Every installed copy finds its
# update by the name in release.json, not by a name of its own, so these are
# also the names the website's download buttons look for. (Up to 0.6.1 they
# were Hyperview.exe and Hyperview-Server.exe; the files on this computer
# can be called anything - they are matched to the manifest by fingerprint.)
APP_FILE = "ExcaliburView.exe"
SERVER_FILE = "ExcaliburView-Server.exe"
MANIFEST = "release.json"

# Under every release on the page, for whoever downloads from it.
HOW_TO = """

---

**ExcaliburView.exe**: the desktop app. Double-click it on each computer. It installs itself for that person, opens, and finds the office's server by itself.

**ExcaliburView-Server.exe**: the Office server. Double-click it once on the computer that should hold the company's drawings. Windows asks for permission once. After that it runs by itself, and keeps itself and every copy of Excalibur View in the office up to date.

The first person to open Excalibur View after the server is up sets it up and gets a join code for everybody else."""

# ---- signing ---------------------------------------------------------------

def load_key(path):
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
    lines = open(path, encoding="utf-8").read().split("\n")
    name, seed = lines[0].strip(), lines[1].strip()
    return name, Ed25519PrivateKey.from_private_bytes(bytes.fromhex(seed))


def payload(release):
    return (
        "hyperview-release-v1\n"
        f"version={release['version']}\n"
        f"channel={release['channel']}\n"
        f"platform={release['platform']}\n"
        f"bytes={release['bytes']}\n"
        f"digest={release['digest'].lower()}\n"
        f"minimum_api_version={release['minimum_api_version']}\n"
    ).encode("utf-8")


def describe(path, version, channel, platform, notes, published, min_api):
    data = open(path, "rb").read()
    return {
        "version": version,
        "channel": channel,
        "platform": platform,
        "published": published,
        "notes": notes,
        # What the file is called on the release, which is what a server
        # looks it up by. The files uploaded must carry these names.
        "download": APP_FILE if platform == APP_PLATFORM else SERVER_FILE,
        "bytes": len(data),
        "digest": hashlib.sha256(data).hexdigest(),
        "signature": "",
        "key": "",
        "minimum_api_version": min_api,
    }


def sign_release(release, key_name, key):
    release = dict(release)
    release["key"] = key_name
    release["signature"] = key.sign(payload(release)).hex()
    return release


def verify(release, public):
    public.verify(bytes.fromhex(release["signature"]), payload(release))


def cmd_sign(a):
    name, key = load_key(a.key)
    notes = open(a.notes_file, encoding="utf-8").read().strip() if a.notes_file else ""
    published = a.published or datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    app = sign_release(describe(a.app, a.version, a.channel, APP_PLATFORM, notes, published, a.min_api), name, key)
    manifest = {"app": app}
    if a.server:
        manifest["server"] = sign_release(
            describe(a.server, a.version, a.channel, SERVER_PLATFORM, notes, published, a.min_api), name, key)
    for part in manifest.values():
        verify(part, key.public_key())
    with open(a.out, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
    print(f"signed {a.version} ({a.channel}) with '{name}' -> {a.out}")


# ---- licenses --------------------------------------------------------------

LICENSE_FIELDS = ("id", "company", "edition", "users", "updates_through", "issued", "note")

# Each product's licenses start their signed text with a line of their own, so
# a license for one can never be taken for a license for another - even though
# the same key signs them all. Excalibur View's was first and keeps its line.
PRODUCTS = {
    "view": {"domain": "excalibur-license-v1", "extension": "evlicense", "id": "evl_",
             "editions": ("office", "sealed")},
    "citadel": {"domain": "excalibur-citadel-license-v1", "extension": "ctlicense", "id": "ctl_",
                "editions": ("standard",)},
}


def license_payload(lic, product="view"):
    lines = [PRODUCTS[product]["domain"]] + [f"{k}={lic[k]}" for k in LICENSE_FIELDS]
    return ("\n".join(lines) + "\n").encode("utf-8")


def make_license(key_path, company, product="view", edition=None, users=0, updates_through=None,
                 years=1, founding=False, note=None, id=None, issued=None):
    """Signs a license and hands it back. The one place a license is made:
    `sign-license` and the Square side both come through here."""
    import secrets
    name, key = load_key(key_path)
    today = datetime.date.today()
    kind = PRODUCTS[product]
    edition = edition or kind["editions"][0]
    if edition not in kind["editions"]:
        raise SystemExit(f"{product} has no edition called {edition}: {', '.join(kind['editions'])}")
    if founding:
        users, through, note = 0, FOREVER, note or "Founding customer"
    else:
        note = note or ""
        if updates_through:
            through = updates_through
        else:
            through = a_year_on(today, years)
    if through != FOREVER:
        datetime.date.fromisoformat(through)
    for text in (company, note):
        if "\n" in text or "\r" in text:
            raise SystemExit("a license cannot have a line break in it")
    if users < 0:
        raise SystemExit("--users is a number; 0 means no limit")
    lic = {
        "id": id or kind["id"] + secrets.token_hex(6),
        "company": company.strip(),
        "edition": edition,
        "users": users,
        "updates_through": through,
        "issued": issued or today.isoformat(),
        "note": note,
        "key": name,
    }
    signed = license_payload(lic, product)
    signature = key.sign(signed)
    key.public_key().verify(signature, signed)
    lic["signature"] = signature.hex()
    return lic


FOREVER = "forever"


def a_year_on(day, years=1):
    """The same date `years` later, as text. 29 February lands on the 28th."""
    try:
        return day.replace(year=day.year + years).isoformat()
    except ValueError:
        return day.replace(day=28).replace(year=day.year + years).isoformat()


def long_date(day):
    """2027-10-01 as "Oct 1, 2027", the way the program writes it."""
    months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
    try:
        d = datetime.date.fromisoformat(day)
    except ValueError:
        return day
    return f"{months[d.month - 1]} {d.day}, {d.year}"


def license_file_name(lic, product="view"):
    """`Mesa Fab, Inc.evlicense` - the company, as a file name."""
    safe = "".join(c for c in lic["company"] if c not in '\\/:*?"<>|').strip().rstrip(".") or lic["id"]
    return f"{safe}.{PRODUCTS[product]['extension']}"


def write_license(lic, path):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(lic, f, indent=2)
        f.write("\n")
    return path


def cmd_sign_license(a):
    lic = make_license(
        a.key, a.company, product=a.product, edition=a.edition, users=a.users,
        updates_through=a.updates_through, years=a.years, founding=a.founding,
        note=a.note, id=a.id, issued=a.issued,
    )
    out = a.out or license_file_name(lic, a.product)
    write_license(lic, out)
    users_said = "every user" if lic["users"] == 0 else f"{lic['users']} users"
    print(f"licensed {lic['company']}: {lic['edition']}, {users_said}, "
          f"updates through {lic['updates_through']} -> {out}")


# ---- plugins ---------------------------------------------------------------

PLUGIN_MAGIC = b"HVPLUGIN1"


def plugin_payload(plugin_id, version, sha256):
    return f"hyperview-plugin/1\n{plugin_id}\n{version}\n{sha256.lower()}\n".encode("utf-8")


def cmd_sign_plugin(a):
    name, key = load_key(a.key)
    wasm = open(a.wasm, "rb").read()
    if not wasm.startswith(b"\0asm"):
        raise SystemExit(f"{a.wasm} is not a WebAssembly file")
    if not re.fullmatch(r"[A-Za-z0-9_-]+", a.id):
        raise SystemExit("a plugin id is letters, digits and dashes")
    sha = hashlib.sha256(wasm).hexdigest()
    signed = plugin_payload(a.id, a.version, sha)
    signature = key.sign(signed)
    key.public_key().verify(signature, signed)
    header = {
        "id": a.id, "version": a.version, "name": a.name or a.id,
        "sha256": sha, "bytes": len(wasm), "key": name, "signature": signature.hex(),
    }
    out = a.out or f"{a.id}-{a.version}.hvplugin"
    with open(out, "wb") as f:
        f.write(PLUGIN_MAGIC + b"\n" + json.dumps(header, separators=(",", ":")).encode("utf-8") + b"\n" + wasm)
    print(f"sealed {a.id} {a.version} with '{name}' -> {out}")


# ---- GitHub ----------------------------------------------------------------

class GitHub:
    """GitHub's API, patient with a home internet connection.

    A connection that drops, times out or is reset is tried again, a few
    times, with a pause that grows; so is anything GitHub answers with a
    5xx. What cannot be fixed by waiting - a bad token, a refused request -
    stops at once, in words."""

    TRIES = 5

    def __init__(self, token_file, repo):
        self.token = open(token_file, encoding="utf-8-sig").read().strip()
        self.repo = repo

    def call(self, method, url, body=None, data=None, kind="application/json", accept="application/vnd.github+json",
             timeout=600):
        if not url.startswith("http"):
            url = f"https://api.github.com/repos/{self.repo}{url}"
        headers = {"Authorization": "Bearer " + self.token, "Accept": accept,
                   "User-Agent": "excalibur-publish", "X-GitHub-Api-Version": "2022-11-28"}
        if body is not None:
            data = json.dumps(body).encode("utf-8")
        if data is not None:
            headers["Content-Type"] = kind
        for attempt in range(1, self.TRIES + 1):
            request = urllib.request.Request(url, data=data, method=method, headers=headers)
            try:
                with urllib.request.urlopen(request, timeout=timeout) as response:
                    raw = response.read()
                    if accept.startswith("application/vnd.github") and raw:
                        return json.loads(raw)
                    return raw
            except urllib.error.HTTPError as e:
                if e.code == 404 and method == "GET":
                    return None
                said = e.read()[:400]
                if e.code >= 500 and attempt < self.TRIES:
                    self.wait(attempt, f"GitHub answered {e.code}")
                    continue
                raise GitHubRefused(e.code, f"GitHub said {e.code} to {method} {url}: {said!r}")
            except (urllib.error.URLError, TimeoutError, ConnectionError, OSError) as e:
                if attempt < self.TRIES:
                    self.wait(attempt, f"the connection to GitHub dropped ({e})")
                    continue
                raise SystemExit(f"Could not reach GitHub after {self.TRIES} tries: {e}")

    @staticmethod
    def wait(attempt, why):
        pause = 5 * attempt
        print(f"  {why}; trying again in {pause} seconds...")
        sys.stdout.flush()
        time.sleep(pause)

    def release(self, tag):
        """The release for a tag, drafts included: a draft left by an upload
        that did not finish is picked up and finished rather than a second
        one made beside it. (GitHub's look-up by tag does not see drafts.)"""
        found = self.call("GET", f"/releases/tags/{tag}")
        if found:
            return found
        for page in range(1, 6):
            listed = self.call("GET", f"/releases?per_page=100&page={page}") or []
            for release in listed:
                if release.get("tag_name") == tag:
                    return release
            if len(listed) < 100:
                break
        return None

    def assets(self, release):
        return self.call("GET", f"/releases/{release['id']}/assets?per_page=100") or []

    def replace_asset(self, release, path, name=None):
        """Uploads a file as `name`, replacing one already there. A try cut
        off part way can leave a half-made asset of that name behind, which
        GitHub then refuses to upload over; it is cleared first each time."""
        name = name or os.path.basename(path)
        data = open(path, "rb").read()
        upload = release["upload_url"].split("{")[0] + "?name=" + urllib.request.quote(name)
        for attempt in range(1, self.TRIES + 1):
            for asset in self.assets(release):
                if asset["name"] == name:
                    self.call("DELETE", f"/releases/assets/{asset['id']}")
            print(f"  uploading {name} ({len(data) / 1048576:.1f} MB)...")
            sys.stdout.flush()
            try:
                self.call("POST", upload, data=data, kind="application/octet-stream", timeout=1800)
            except GitHubRefused as e:
                if e.code == 422 and attempt < self.TRIES:
                    self.wait(attempt, "GitHub still had part of an earlier try")
                    continue
                raise SystemExit(str(e))
            print(f"  uploaded {name}")
            return
        raise SystemExit(f"{name} did not upload after {self.TRIES} tries.")


class GitHubRefused(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def cmd_upload(a):
    gh = GitHub(a.token_file, a.repo)
    manifest = json.load(open(a.files[0], encoding="utf-8"))
    channel = manifest["app"]["channel"]
    notes = open(a.notes_file, encoding="utf-8").read().strip() if a.notes_file else manifest["app"]["notes"]
    tag = f"v{a.version}"

    # Each program goes up under the name release.json gives it, matched by
    # fingerprint, so a file on this computer can be called anything and the
    # manifest and the release can never disagree.
    wanted = {part["digest"]: part["download"] for part in manifest.values()}
    uploads = []
    for path in a.files[1:]:
        digest = hashlib.sha256(open(path, "rb").read()).hexdigest()
        if digest not in wanted:
            raise SystemExit(f"{path} is not one of the programs in {a.files[0]} - nothing was uploaded.")
        uploads.append((path, wanted.pop(digest)))
    if wanted:
        raise SystemExit(f"{a.files[0]} also lists {', '.join(wanted.values())}, which was not given - nothing was uploaded.")

    release = gh.release(tag)
    if release is None:
        # A draft first, so no server sees a release with half its files.
        release = gh.call("POST", "/releases", {
            "tag_name": tag, "target_commitish": "main", "name": f"Excalibur View {a.version}",
            "body": notes + HOW_TO, "draft": True, "prerelease": channel != "stable",
        })
    elif release.get("draft"):
        print(f"finishing {tag}, which an earlier try left as a draft")
    for path, name in uploads:
        gh.replace_asset(release, path, name)
    gh.replace_asset(release, a.files[0], MANIFEST)

    # Every file there, whole, before anybody can see the release.
    there = {x["name"]: x for x in gh.assets(release)}
    for path, name in uploads + [(a.files[0], MANIFEST)]:
        asset = there.get(name)
        if not asset or asset.get("state") != "uploaded" or asset.get("size") != os.path.getsize(path):
            raise SystemExit(f"{name} is not on the release in full - it stays a draft. Running this again is safe.")
    gh.call("PATCH", f"/releases/{release['id']}", {
        "name": f"Excalibur View {a.version}", "draft": False, "prerelease": channel != "stable",
        "body": notes + HOW_TO, "make_latest": "true" if channel == "stable" else "false",
    })
    print(f"published {tag} as {'a full release' if channel == 'stable' else 'an early release'}: "
          f"{', '.join(name for _, name in uploads)}")


def cmd_promote(a):
    gh = GitHub(a.token_file, a.repo)
    name, key = load_key(a.key)
    tag = f"v{a.version}"
    release = gh.release(tag)
    if release is None:
        raise SystemExit(f"there is no release {tag}")
    asset = next((x for x in release["assets"] if x["name"] == MANIFEST), None)
    if asset is None:
        raise SystemExit(f"{tag} has no {MANIFEST}")
    raw = gh.call("GET", f"/releases/assets/{asset['id']}", accept="application/octet-stream")
    manifest = json.loads(raw)
    for part in list(manifest):
        release_part = dict(manifest[part])
        release_part["channel"] = "stable"
        manifest[part] = sign_release(release_part, name, key)
        verify(manifest[part], key.public_key())
    out = os.path.join(os.path.dirname(os.path.abspath(a.key)), f"release-{a.version}-stable.json")
    with open(out, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
    gh.replace_asset(release, out, MANIFEST)
    gh.call("PATCH", f"/releases/{release['id']}", {"prerelease": False, "make_latest": "true"})
    print(f"promoted {tag}: every office now takes it")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="command", required=True)
    s = sub.add_parser("sign")
    s.add_argument("--key", required=True)
    s.add_argument("--version", required=True)
    s.add_argument("--channel", choices=["preview", "stable"], default="preview")
    s.add_argument("--notes-file")
    s.add_argument("--app", required=True)
    s.add_argument("--server")
    s.add_argument("--out", default=MANIFEST)
    s.add_argument("--published")
    s.add_argument("--min-api", type=int, default=1)
    u = sub.add_parser("upload")
    u.add_argument("--token-file", required=True)
    u.add_argument("--repo", required=True)
    u.add_argument("--version", required=True)
    u.add_argument("--notes-file")
    u.add_argument("files", nargs="+", help="release.json first, then the program files")
    r = sub.add_parser("promote")
    r.add_argument("--key", required=True)
    r.add_argument("--token-file", required=True)
    r.add_argument("--repo", required=True)
    r.add_argument("--version", required=True)
    l = sub.add_parser("sign-license")
    l.add_argument("--key", required=True)
    l.add_argument("--company", required=True)
    l.add_argument("--product", choices=sorted(PRODUCTS), default="view",
                   help="which program the license is for; each has its own signed first line")
    l.add_argument("--edition", help="view: office (the default) or sealed; citadel: standard")
    l.add_argument("--users", type=int, default=0, help="0 means no limit")
    l.add_argument("--years", type=int, default=1, help="years of updates from today")
    l.add_argument("--updates-through", help="YYYY-MM-DD, or forever")
    l.add_argument("--founding", action="store_true", help="every user, updates forever")
    l.add_argument("--note")
    l.add_argument("--id")
    l.add_argument("--issued")
    l.add_argument("--out")
    g = sub.add_parser("sign-plugin")
    g.add_argument("--key", required=True)
    g.add_argument("--id", required=True)
    g.add_argument("--version", required=True)
    g.add_argument("--name")
    g.add_argument("--out")
    g.add_argument("wasm")
    a = p.parse_args()
    try:
        {"sign": cmd_sign, "upload": cmd_upload, "promote": cmd_promote,
         "sign-plugin": cmd_sign_plugin, "sign-license": cmd_sign_license}[a.command](a)
    except GitHubRefused as e:
        raise SystemExit(str(e))


if __name__ == "__main__":
    main()
