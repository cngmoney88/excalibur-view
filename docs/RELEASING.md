# Releasing Hyperview

**Distribution is on hold.** Updates will be handled by a fleet management app
rather than by a public release feed, so nothing here builds or publishes
anything yet.

What is built, and stays built whatever ships the bytes, is the part that makes
an update safe to install: a signed manifest, and a seat that checks it against
keys compiled into itself. A fleet app is another way of getting the file to a
machine. It is not a reason to trust the file.

## The contract a fleet app has to satisfy

Whatever ends up distributing builds, a seat will install one only if all of
this holds.

**The manifest.** JSON, as `hub::update::Release`:

```json
{
  "version": "1.5.0",
  "channel": "stable",
  "platform": "windows-x64",
  "published": "2026-09-18T12:00:00Z",
  "notes": "Dynamic Fill.",
  "download": "/api/v1/update/1.5.0/download?platform=windows-x64",
  "bytes": 48234112,
  "digest": "f42562cf…",
  "signature": "a41c…",
  "key": "mesafab-2026",
  "minimum_api_version": 1
}
```

**What is signed** is not the JSON. Two encoders will not agree on key order or
number formatting, and a signature that depends on how something was printed is
not a signature. It is exactly these bytes:

```
hyperview-release-v1\n
version=<version>\n
channel=<stable|preview>\n
platform=<platform>\n
bytes=<length>\n
digest=<sha256, lower-case hex>\n
minimum_api_version=<n>\n
```

Ed25519 over that, hex-encoded. Release notes and the published date are
deliberately **not** signed: they are presentation, and changing them should not
invalidate a build.

**What a seat checks**, in this order, before anything runs:

1. The key named in `key` is in the list compiled into that build.
2. The SHA-256 of what actually arrived matches `digest`.
3. The length matches `bytes`.
4. The signature verifies against the signing payload above.

Both halves matter. The digest says the download is whole; the signature says
the publisher meant to publish it. A download that matches its digest and
nothing else proves only that whoever wrote the digest also wrote the file.

**When it installs:** on the next launch. Never mid-session. Somebody three
hours into a takeoff does not get restarted.

## The tool

`hyperview-release` does the signing and the checking, and a fleet app can call
it or reimplement it against the contract above.

```
hyperview-release keygen mesafab-2026
hyperview-release public --key hyperview-signing.key
hyperview-release sign   --key hyperview-signing.key --version 1.5.0 \
                         --platform windows-x64 --notes "Dynamic Fill." \
                         Hyperview-1.5.0-Setup.exe
hyperview-release check  Hyperview-1.5.0-Setup.exe manifest.json
```

`check` verifies against the keys **the program** trusts, not against whatever
signed it, so it says what a seat will say.

## The key

Generated on the publisher's own machine, by the publisher. It is the only copy.
Back it up somewhere only they can reach — a password manager or a safe, not the
repository and not a shared drive.

Anyone holding it can ship software to every machine running Hyperview. Losing
it means no future release can be signed by that key; seats go on running what
they have, and a new key is added alongside the old one rather than replacing
it.

The public half goes in `crates/hyperview/src/trust.rs`. An empty list there
means the build refuses every update and says so, which is the right way round:
a program that installs anything when it has been told to trust nothing is worse
than one that installs nothing.

## What the server already does

The Hyperview server has the distribution end of this, and a fleet app can drive
it or replace it:

| | |
|---|---|
| `POST /api/v1/admin/releases` | installer + signed manifest. Refuses one that does not match its manifest, or one with no signature at all. Holds no signing key and signs nothing. |
| `POST /api/v1/admin/releases/{version}/ready` | offer it to the office, or take it back. A release arrives unoffered so one seat can try it first. |
| `POST /api/v1/admin/pin` | hold every seat on one version. A fabricator halfway through a bid package should not have the tool change under them. |
| `GET /api/v1/update/latest` | what a seat asks. |

## The installer

`installer/hyperview.iss` builds it with Inno Setup. It puts the program, the
pdfium library and a `profiles` folder on a machine, and registers Hyperview as
*a* program that opens PDFs rather than taking the association away from
whatever the person already uses.
