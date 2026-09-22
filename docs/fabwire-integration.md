# FabWire and Excalibur Hyperview

FabWire used to talk to Bluebeam Studio in `routes/bluebeam.js`: push a bid's
drawings into a Studio project, take off in Revu, pull the markups back through
`mapBB` into the member list. As of Hyperview 0.5.5 FabWire does the same
through `routes/hyperview.js`, against the office's own Hyperview server, and
Bluebeam is retired (its card is still under Settings → Takeoff → Integrations,
folded away, for a job that is mid-flight in Revu).

## What changed and what did not

| Bluebeam Studio | Hyperview |
|---|---|
| OAuth app, `.bluebeam.enc`, a refresh every 3 days or it dies | One key, made once by signing in. It does not expire; **Take back** in Hyperview stops it at once. |
| `ensureSharedProject` / `ensureBidFolder` → `Bids/<year>/<bid>`, duplicate folders when the root id changed | `POST /api/v1/projects` with `reference: "fabwire:bid:<id>"`. Asking twice gives the same project — a retried push cannot make a second one. |
| `uploadPdf` (S3 URL, then confirm) | `POST /api/v1/sets`, one multipart request per PDF. The same bytes twice are one drawing (`already_here`). |
| Addenda: a second file beside the first, takeoff started over | A drawing issued again under the same name **supersedes** the old one; with `carry_forward` the takeoff comes across, each markup marked **Carried forward — check**, and never onto a sheet that changed size. |
| `markuplist` job, polled up to 90 times | `GET /api/v1/projects/{id}/markuplist` — one request, answered at once, current issues only (`?all=1` for every issue). |
| `mapBB(results, …)` | **Unchanged.** Same `results`, same `Markups`, `Subject`, `Comment`, `ExtendedProperties`. Plus a `Hyperview` block with exact numbers (below). |
| Counts guessed from the text | `Hyperview.Each`: a count markup's real count, times its quantity. Seven moment connections are 7. |
| "Is the takeoff still the one I pulled?" — pull again and diff | `GET /api/v1/projects/{id}/stamp`: a short fingerprint that moves when any markup on a current set does. FabWire shows "changed since you pulled" without pulling. |
| `expire-for-bid` deleted the Studio project | `POST /api/v1/projects/{id}/archive` puts it away. Nothing is destroyed; drawings, markups and takeoff stay, and it can be brought back. |

## Connecting

FabWire → **Settings → Takeoff → Integrations → Hyperview**. Give the address
(`http://127.0.0.1:8714` when FabWire runs on the same machine as the Hyperview
server) and either a Hyperview administrator's email and password — used once
to make a key named `FabWire`, and not kept — or a key made by hand in
Hyperview under **Studio → Office → Other programs**. The key is kept in
`.hyperview.enc`; the bid ↔ project links in `.hyperview-links.enc`. Both are
in the backup list.

FabWire refuses a server older than 0.5.5 (`MIN_SERVER` in `routes/hyperview.js`)
and says so, rather than half-working against one that lacks the calls above.

## FabWire's endpoints

All behind FabWire's own sign-in. None of them ever answers 401 — FabWire's
`apiFetch` signs the person out on any 401, and a Hyperview problem is not a
reason to sign somebody out of FabWire.

| | |
|---|---|
| `GET /api/hyperview/status` | Connected or not, to what, as whom |
| `GET /api/hyperview/preflight` | Health, version, sign-in and the shared tool chest, checked |
| `POST /api/hyperview/connect` · `/disconnect` | As above |
| `GET /api/hyperview/job/:jobId` | A job's Hyperview project, when it has one |
| `GET /api/hyperview/bid/:bidId` | The bid's project, its sets, and a link to open it |
| `GET /api/hyperview/bid/:bidId/changes` | Stamp now vs stamp at the last pull |
| `POST /api/hyperview/bid/:bidId/push-folder` | Every PDF in the bid's `01 Drawings`, server-side |
| `POST /api/hyperview/bid/:bidId/push` | Chosen files (addenda). Verified and reconciled after. |
| `POST /api/hyperview/bid/:bidId/pull` | The takeoff, as a background job (Cloudflare's 100-second limit) |
| `POST /api/hyperview/expire-for-bid` | Called by bid activation: archives the project, keeps the link |
| `/api/hyperview/aliases…` | The alias table, shared with the Bluebeam routes (`routes/_takeoff-aliases.js`) |

## The markup list

```
GET {HYPERVIEW_URL}/api/v1/projects/prj_…/markuplist
→ { "project": "prj_…", "stamp": "3f1c…", "archived": null, "carriedForward": 2,
    "results": [ { "file": "S-101 Framing Plans", "set": "set_…",
      "markups": { "Markups": [
        { "Subject": "W12x26", "Comment": "24'-6\"", "Status": "",
          "ExtendedProperties": { "LBS Per FT": "26.00" },
          "Hyperview": { "MarkupId": "…", "Set": "set_…", "PageIndex": 0, "Kind": "Length",
                         "Scaled": true, "Quantity": 2, "Each": 2, "LengthFt": 24.5,
                         "AreaSqFt": null, "VolumeCuFt": null,
                         "CarriedForward": false, "NeedsCheck": false } } ],
        "Warning": null } } ],
    "superseded": [ { "file": "S-101 Framing Plans", "set": "set_old…", "supersededBy": "set_…", "markups": 14 } ] }
```

`LengthFt` is the markup's own length; `Each` is how many it stands for (a
quantity of 2 on a 24'-6" beam is two beams). `mapBB` reads the `Hyperview`
block when it is there: exact counts (including
connection types routed to `erct.connQtys`) and exact lengths. Carried-forward
markups not yet checked are counted and shown above the import preview, and
anything it cannot place is listed as left out, with the file and sheet and a
link to open it, rather than dropped.

A measurement on a sheet with no scale is left out of every total and
`Warning` says so. FabWire shows it rather than importing a short takeoff
silently.

## Opening a drawing from FabWire

Each set and each left-out row carries `hyperview://open?set=<id>&page=<n>`.
Hyperview registers that link for the person when it installs; clicking it
opens the set at that sheet in the window already open, or starts Hyperview.

The whole API is described at `GET {HYPERVIEW_URL}/api/v1/openapi.json`.
