# Excalibur licensing standard

For every Excalibur Construction Technologies app that is sold per office: Excalibur View Office today, Excalibur Citadel next. Written from what Excalibur View 0.6.2 does, so both apps behave the same and one signing tool issues both.

**The rule under all of it: a license can pause sharing new work, and nothing else.** It never deletes anything, never locks anyone out of what is already there, never signs anybody out, and never phones home.

## 1. How it works, in one paragraph

The office buys a number of users and a year of updates. We send a small signed file (`Company Name.ctlicense` for Citadel). An administrator adds it on the app's settings page, or drops it in the server's data folder. The server checks the signature itself, offline, against a public key built into the program. A new install runs everything for 30 days first. When a trial ends, or a license stops covering the version installed, everyone can still open, search, print and export everything; only creating and changing things pauses until a license is added.

## 2. The license file

JSON, UTF-8. Every field is required.

| Field | Example | Meaning |
|---|---|---|
| `id` | `ctl_3f9a1c2b7d4e` | Our reference. `ctl_` for Citadel, `evl_` for View. |
| `company` | `Acme Builders, LLC` | Printed on the license and shown to administrators. |
| `edition` | `standard` | Citadel has one edition for now. An unknown edition is refused. |
| `users` | `12` | How many user accounts it covers. `0` means no limit. |
| `updates_through` | `2027-10-01` or `forever` | The last day whose releases it covers. |
| `issued` | `2026-10-01` | The day it was signed. |
| `note` | `Founding customer` | Free text, may be empty. |
| `key` | `mesafab-2026` | Which trusted key signed it. |
| `signature` | 128 hex characters | Ed25519 signature of the signed text below. |

## 3. The signed text (must match byte for byte)

Eight lines, each ending in `\n` (LF, not CRLF), UTF-8, no trailing spaces:

```
excalibur-citadel-license-v1
id=<id>
company=<company>
edition=<edition>
users=<users>
updates_through=<updates_through>
issued=<issued>
note=<note>
```

The first line is what keeps products apart. The same key signs View and Citadel licenses, and View's first line is `excalibur-license-v1`. Because the first line is part of what is signed, a View license can never be accepted as a Citadel license, or the reverse. Every new product gets its own first line: `excalibur-<product>-license-v1`.

A field containing a line break is refused before anything else is checked. Otherwise someone could add lines to the signed text.

## 4. Keys

- **Trusted public key**, built into the program. This is the same release key Excalibur View trusts:
  `mesafab-2026` = `14d93acf4729859a3a7b01f458e118173a7a1859305193e84c3bd7513eb72790`
- **The private key** lives only on Creede's PC (`ExcaliburSigning\hyperview-signing.key`). It is never copied into a repo, a server, a CI system or a chat.
- **Never add a test key to the program's trusted list.** Tests pass their own trusted list into the checker (see section 9).
- **A key that has signed licenses is never removed from the list.** Removing it would invalidate every license it signed.

## 5. Checking a license (reference code, Node)

Drop-in for a Node server. It uses only the built-in `crypto` module, and was tested against files the signing tool made.

```js
'use strict';
// Checks an Excalibur Citadel license file (.ctlicense) offline.
// Same scheme as Excalibur View's .evlicense; only the first signed line differs.
const crypto = require('crypto');

const DOMAIN = 'excalibur-citadel-license-v1';
const FIELDS = ['id', 'company', 'edition', 'users', 'updates_through', 'issued', 'note'];
const EDITIONS = ['standard'];
const FOREVER = 'forever';

// Public keys the program trusts, by name. Only public halves ever go in here.
// The real one is Excalibur's release key (the same one Excalibur View trusts).
const TRUSTED = {
  'mesafab-2026': '14d93acf4729859a3a7b01f458e118173a7a1859305193e84c3bd7513eb72790',
};

const SPKI_ED25519 = Buffer.from('302a300506032b6570032100', 'hex');
const isDate = (s) => typeof s === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(s) && !isNaN(Date.parse(s + 'T00:00:00Z'));

function payload(lic) {
  return Buffer.from([DOMAIN, ...FIELDS.map((k) => `${k}=${lic[k]}`)].join('\n') + '\n', 'utf8');
}

// Returns the license, or throws an Error whose message a person can read.
function readLicense(text, trusted = TRUSTED) {
  let lic;
  try { lic = JSON.parse(String(text).replace(/^﻿/, '')); } catch { throw new Error('That file is not a license.'); }
  for (const k of [...FIELDS, 'key', 'signature']) {
    if (lic[k] === undefined || lic[k] === null) throw new Error(`That license is missing ${k}.`);
  }
  for (const k of FIELDS) {
    if (/[\r\n]/.test(String(lic[k]))) throw new Error('That license has a line break in it, so it is not one.');
  }
  if (!Number.isInteger(lic.users) || lic.users < 0) throw new Error('That license has no proper number of users.');
  if (!EDITIONS.includes(lic.edition)) throw new Error(`That license is for ${lic.edition}, not this program.`);
  if (!(lic.updates_through === FOREVER || isDate(lic.updates_through)) || !isDate(lic.issued)) {
    throw new Error('That license has a date that is not a date.');
  }
  const pub = trusted[lic.key];
  if (!pub) throw new Error('That license was not signed by Excalibur Construction Technologies.');
  const key = crypto.createPublicKey({ key: Buffer.concat([SPKI_ED25519, Buffer.from(pub, 'hex')]), format: 'der', type: 'spki' });
  const sig = /^[0-9a-f]{128}$/i.test(lic.signature) ? Buffer.from(lic.signature, 'hex') : null;
  if (!sig || !crypto.verify(null, payload(lic), key, sig)) {
    throw new Error('That license does not check out: it was changed after it was signed, or it is for another program.');
  }
  return lic;
}

// Whether a version published on `published` (YYYY-MM-DD or an ISO time) comes with this license's updates.
function covers(lic, published) {
  if (lic.updates_through === FOREVER) return true;
  const day = String(published || '').slice(0, 10);
  return isDate(day) ? day <= lic.updates_through : true;
}

module.exports = { readLicense, covers, payload, DOMAIN, FOREVER };
```

- **Check it every time you use it**, not once when it is added. Store the license text and verify it on each state check (it takes microseconds). A "licensed" flag written into the database by hand must never count.
- **The error messages are for people.** Show them as they are.

## 6. States, and what each one allows

Work the state out on the server from three stored settings (`licensing_began`, `founding_install`, `license`) and the clock.

| State | When | Sharing new work | Reading / exporting |
|---|---|---|---|
| **founding** | This install was in use before licensing existed (section 7) | Yes, every user, every update, for good | Yes |
| **licensed** | A license that checks out, covering this version | Yes, up to its user count | Yes |
| **trial** | No license; first 30 days from the first start | Yes, any number of users | Yes |
| **unlicensed** | No license; trial over | **Paused** | Yes |
| **beyond** | A license, but this version was built after its `updates_through` (installed by hand) | **Paused** until they renew or put back a covered version | Yes |

- **Trial clock.** It starts the first time the licensing version runs. A clock turned back gives the full trial; it never ends one early.
- **Where it is enforced:** the server, in the one function every write goes through. A refused write answers **HTTP 403** with `{"error":"license_needed","message":"…"}`. Never 401: clients sign people out on 401, and a license problem is never a reason to sign someone out.
- **What is never blocked:** signing in, reading, searching, viewing, printing, exporting, downloading files, and the administrator's settings page. The administrator must always be able to add a license.
- **User limit.** It is checked only when a new account is added, or a join code is used. People already signed in are never turned away, and nobody is removed.
- **Updates.** The server only auto-updates to releases published on or before `updates_through`. The version on the machine keeps working forever.

## 7. Founding installs (Arc Valley, Mesa Fab)

An install that already had people on it before licensing shipped is **founding**, automatically and permanently:

- The first time a licensing version starts, it records `licensing_began = now`.
- If the earliest account's `created` is before the founding date, it also records `founding_install` and never asks for a license again.
- The founding date for View is `2026-10-01T00:00:00Z`. Use the date Citadel's licensing ships.
- A fresh install has nobody on it when licensing first starts, so it gets the trial, whatever its clock says.

Arc Valley's production Citadel must come out founding. Check that before shipping.

As a backup for a rebuilt server, a founding license file is also issued: `users 0`, `updates_through forever`, `note Founding customer`.

## 8. What people see

- **Administrators** see a single line of status on the settings page: *Office trial: 12 days left*, *Licensed to Acme Builders, LLC · 12 users · updates through Oct 1, 2027*, or *Founding install: every user and every update, for good*. The page has an "Add license file…" button, a "Buy" or "Renew or add users" link to `https://excaliburct.com/pricing/#buy`, and "Copy license id".
- **Everyone else** sees nothing about licensing, unless sharing is paused. Then they see one line: *Sharing is paused. Your administrator has been told.*
- **Heads-up.** Administrators get a notice 30 days before `updates_through`, and after it passes: *Updates on your license ended on Oct 1, 2027. Everything keeps working on the version you have; renew to get newer ones.*
- **When a trial ends:** *Your trial has ended. Nothing is deleted: everyone can still open and export everything. Sharing new work is paused until you add a license.*
- **Never:** a pop-up, a countdown shown to people who can't act on it, a watermark, or a disabled export.

## 9. Tests the app must have

1. A license signed by the tool checks out (use the sample below with the test key).
2. A tampered field is refused (users changed by hand), and so is a line break in a field.
3. An Excalibur View license is refused.
4. A key not on the list is refused.
5. The trial runs 30 days, then unlicensed. Reads still work, writes get 403 `license_needed`, and the user's token still works.
6. A clock turned back never ends the trial early.
7. An install with people from before the founding date becomes founding, and stays founding across restarts.
8. The user limit applies only to adding people.
9. A build dated after `updates_through` pauses sharing. A build on the last day is covered.
10. A license file left in the data folder is picked up at start.

**Test key** (for tests only, never in the product's list): name `test-2026`, seed `09` repeated 32 times, public key `fd1724385aa0c75b64fb78cd602fa1d991fdebf76b13c58ed702eac835e9f618`. Pass it as the trusted list in tests: `readLicense(text, { 'test-2026': 'fd17…f618' })`.

Sample licenses signed with it by the real tool:

```json
{
  "id": "ctl_000000000001",
  "company": "Acme Builders, LLC",
  "edition": "standard",
  "users": 12,
  "updates_through": "2027-10-01",
  "issued": "2026-10-01",
  "note": "",
  "key": "test-2026",
  "signature": "df6bfd8a68edefe735b85373f7138eb52571a70a417a9bea239cea534e19fade686beadb1f5ab42df01608ccb9c9f0569c662c5f3f7f6bfbcbd8d9eeabacb000"
}
```

```json
{
  "id": "ctl_000000000002",
  "company": "Arc Valley Construction",
  "edition": "standard",
  "users": 0,
  "updates_through": "forever",
  "issued": "2026-10-01",
  "note": "Founding customer",
  "key": "test-2026",
  "signature": "42ec8d8226e489932d26157c6e43c5f0800c586d79b33d770b7819f7a3842b0cd010a482efba7c287c3dde50e17c25b097865c24f8df1d275b4f203d2eb67c04"
}
```

## 10. Issuing one

On Creede's PC, in `ExcaliburSigning`:

```
python publish.py sign-license --key hyperview-signing.key --product citadel --company "Acme Builders, LLC" --users 12 --years 1
python publish.py sign-license --key hyperview-signing.key --product citadel --company "Arc Valley Construction" --founding
```

Each command writes `Acme Builders, LLC.ctlicense` (the file named after the company). The same script with no `--product`, or `--product view`, makes Excalibur View licenses (`.evlicense`).

## Decided by Creede, not the app

- **Citadel's price** per user and per year of updates.
- **Whether portal accounts count as users.** Portal accounts are for subs, owners and architects. Recommended: count only the office's own staff accounts, so the license never charges the office for bringing people in.
