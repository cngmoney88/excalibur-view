#!/usr/bin/env python3
"""Selling Excalibur View Office through Square, from the publisher's own PC.

Square takes the money. A license can only be made here, beside the signing
key, so this script is the bridge: it makes the payment links the website's
Buy buttons point at, and it turns paid orders into signed `.evlicense`
files with a ready-to-send email.

Nothing about a customer is kept anywhere but this computer, and the key is
never sent anywhere.

    python square.py setup                 make (or update) the payment links
    python square.py orders                what has been paid, and what is issued
    python square.py issue                 sign a license for each new order
    python square.py watch                 the same, quietly, for a scheduled task

Beside the signing key it expects:

    square-token.txt    a Square access token (Developer Console -> your app).
                        Permissions: PAYMENTS_READ, ORDERS_READ, ORDERS_WRITE,
                        PAYMENTS_WRITE, MERCHANT_PROFILE_READ.
    square-links.json   written by `setup`: every payment link it has made.
    square-site.json    written by `setup`: the settings to paste into the
                        website's site.json.
    square-orders.json  written by `issue`: which payments already have a
                        license, so nobody is ever charged into two of them.
    licenses/           where the .evlicense files and their emails land.

`issue` never guesses a company name. Square asks the buyer for it at
checkout; if the answer does not come back with the order, that order waits
and says so, and one command with `--company` finishes it.
"""

import argparse
import datetime
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

import publish

# What the payment links sell. The name is what the buyer sees on the
# checkout page and on their receipt, and what `issue` reads back to tell a
# first purchase from a renewal.
SEATS_ITEM = "Excalibur View Office - per user"
RENEWAL_ITEM = "Excalibur View Office updates - per user, one year"
SEATS_PRICE = 25000  # cents
RENEWAL_PRICE = 10000
MOST_SEATS = 25   # a link per user count up to here; past it, they email us
COMPANY_FIELD = "Company name, as it should read on the license"
EMAIL_FIELD = "Administrator's email (where the license goes)"

SQUARE_VERSION = "2026-05-20"
LIVE = "https://connect.squareup.com"
SANDBOX = "https://connect.squareupsandbox.com"


# ---- talking to Square -----------------------------------------------------

class SquareSaid(Exception):
    """Square refused something, in its own words."""


class Square:
    TRIES = 4

    def __init__(self, token, base=LIVE, version=SQUARE_VERSION):
        self.token = token.strip()
        self.base = base.rstrip("/")
        self.version = version

    def call(self, method, path, body=None):
        url = self.base + path
        data = json.dumps(body).encode("utf-8") if body is not None else None
        headers = {
            "Authorization": "Bearer " + self.token,
            "Square-Version": self.version,
            "Accept": "application/json",
            "User-Agent": "excalibur-square",
        }
        if data is not None:
            headers["Content-Type"] = "application/json"
        for attempt in range(1, self.TRIES + 1):
            request = urllib.request.Request(url, data=data, method=method, headers=headers)
            try:
                with urllib.request.urlopen(request, timeout=60) as response:
                    raw = response.read()
                    return json.loads(raw) if raw else {}
            except urllib.error.HTTPError as e:
                said = e.read().decode("utf-8", "replace")
                if e.code >= 500 and attempt < self.TRIES:
                    time.sleep(3 * attempt)
                    continue
                raise SquareSaid(f"Square said {e.code} to {method} {path}: {reason(said)}")
            except (urllib.error.URLError, TimeoutError, OSError) as e:
                if attempt < self.TRIES:
                    time.sleep(3 * attempt)
                    continue
                raise SystemExit(f"Could not reach Square after {self.TRIES} tries: {e}")

    def paged(self, path, key, params=None):
        """Every page of a list endpoint, oldest first where Square allows."""
        out = []
        params = dict(params or {})
        while True:
            query = urllib.parse.urlencode({k: v for k, v in params.items() if v not in (None, "")})
            answer = self.call("GET", f"{path}?{query}" if query else path)
            out.extend(answer.get(key) or [])
            cursor = answer.get("cursor")
            if not cursor:
                return out
            params["cursor"] = cursor


def reason(said):
    """The sentence out of a Square error, when there is one."""
    try:
        errors = json.loads(said).get("errors") or []
        return "; ".join(e.get("detail") or e.get("code", "") for e in errors) or said[:300]
    except Exception:
        return said[:300]


# ---- the publisher's folder ------------------------------------------------

class Folder:
    """The signing folder: the key, the token, and what has been issued."""

    def __init__(self, path):
        self.path = os.path.abspath(path)
        self.key = os.path.join(self.path, "hyperview-signing.key")
        self.token_file = a_file_called(self.path, "square-token")
        self.links_file = os.path.join(self.path, "square-links.json")
        self.issued_file = os.path.join(self.path, "square-orders.json")
        self.licenses = os.path.join(self.path, "licenses")
        self.log_file = os.path.join(self.path, "square-log.txt")

    def token(self):
        if not os.path.exists(self.token_file):
            raise SystemExit(
                f"{self.token_file} is not there. Make an app in Square's Developer Console, "
                "copy its access token into that file, and run this again. (Notepad adds .txt "
                "to what you type, so a file saved as square-token.txt can end up as "
                "square-token.txt.txt - either name works here.)"
            )
        token = open(self.token_file, encoding="utf-8-sig").read().strip()
        if not token:
            raise SystemExit(f"{self.token_file} is empty.")
        return token

    def square(self, sandbox=False):
        return Square(self.token(), SANDBOX if sandbox else LIVE)

    def links(self):
        return read_json(self.links_file, {})

    def save_links(self, links):
        write_json(self.links_file, links)

    def issued(self):
        return read_json(self.issued_file, {})

    def remember(self, payment_id, record):
        issued = self.issued()
        issued[payment_id] = record
        write_json(self.issued_file, issued)

    def say(self, line, quiet=False):
        if not quiet:
            print(line)
        with open(self.log_file, "a", encoding="utf-8") as f:
            f.write(f"{datetime.datetime.now():%Y-%m-%d %H:%M:%S}  {line}\n")


def a_file_called(folder, stem):
    """`square-token.txt` however Windows spelled it.

    Notepad adds `.txt` to a name that already ends in it, so a file saved as
    square-token.txt is often square-token.txt.txt on disk - and with
    Explorer hiding known extensions, both look the same to the person who
    saved it. Anything starting with the stem counts, and the tidiest
    spelling wins.
    """
    wanted = os.path.join(folder, stem + ".txt")
    try:
        here = os.listdir(folder)
    except OSError:
        return wanted
    likely = sorted(
        (f for f in here if f.lower().startswith(stem.lower()) and os.path.isfile(os.path.join(folder, f))),
        key=lambda f: (f.lower() != stem.lower() + ".txt", len(f), f.lower()),
    )
    return os.path.join(folder, likely[0]) if likely else wanted


def read_json(path, empty):
    if not os.path.exists(path):
        return empty
    try:
        return json.loads(open(path, encoding="utf-8-sig").read() or "null") or empty
    except json.JSONDecodeError as e:
        raise SystemExit(f"{path} is not readable ({e}). It has been left alone.")


def write_json(path, value):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(value, f, indent=2, sort_keys=True)
        f.write("\n")


# ---- setup: the payment links ----------------------------------------------

def a_location(square, wanted=None):
    locations = square.call("GET", "/v2/locations").get("locations") or []
    active = [l for l in locations if l.get("status", "ACTIVE") == "ACTIVE"]
    if wanted:
        for l in locations:
            if wanted in (l.get("id"), l.get("name")):
                return l
        raise SystemExit(f"No location called {wanted}. Yours: " + ", ".join(l.get("name", "?") for l in locations))
    if not active:
        raise SystemExit("This Square account has no active location.")
    return active[0]


def link_body(kind, name, price, users, site, note):
    """One payment link, for exactly this many users.

    Square's hosted checkout has no quantity stepper - the count is fixed when
    the link is made - so we make a link per count and the website's own
    stepper sends the buyer to the right one. The idempotency key is worked
    out rather than random, so making them twice asks Square for the same
    links back instead of a second set.
    """
    return {
        "idempotency_key": f"excalibur-{kind}-{price}-{users}",
        "description": note,
        "name": f"{name} x{users}" if users > 1 else name,
        "order": {
            "location_id": None,  # filled in by the caller
            "line_items": [{
                "name": name,
                "quantity": str(users),
                "base_price_money": {"amount": price, "currency": "USD"},
            }],
        },
        "checkout_options": {
            "allow_tipping": False,
            "ask_for_shipping_address": False,
            "enable_coupon": False,
            "enable_loyalty": False,
            "redirect_url": site.rstrip("/") + "/thanks/",
            "custom_fields": [{"title": COMPANY_FIELD}, {"title": EMAIL_FIELD}],
        },
        "payment_note": f"{note} ({users} user{'s' if users > 1 else ''})",
    }


PRODUCTS_TO_SELL = [
    ("seats", SEATS_ITEM, "price",
     "Excalibur View Office, bought once per user. Updates for the first year are included."),
    ("renewal", RENEWAL_ITEM, "renewal",
     "Another year of updates for Excalibur View Office. Everything keeps working either way."),
]


def a_ladder(square, kind, name, price, most, site, note, location, had):
    """The links for 1..most users, making only the ones that are missing."""
    rungs, made, kept = {}, 0, 0
    for users in range(1, most + 1):
        before = had.get(str(users)) or {}
        if before.get("url") and before.get("price") == price and before.get("item") == name:
            rungs[str(users)] = before
            kept += 1
            continue
        body = link_body(kind, name, price, users, site, note)
        body["order"]["location_id"] = location["id"]
        link = (square.call("POST", "/v2/online-checkout/payment-links", body).get("payment_link") or {})
        rungs[str(users)] = {
            "id": link.get("id"),
            "url": link.get("url") or link.get("long_url"),
            "item": name,
            "price": price,
            "users": users,
            "made": datetime.date.today().isoformat(),
        }
        made += 1
    return rungs, made, kept


def cmd_setup(a):
    folder = Folder(a.folder)
    square = folder.square(a.sandbox)
    location = a_location(square, a.location)
    links = folder.links()
    most = max(1, a.most)
    block = {}
    for kind, name, which, note in PRODUCTS_TO_SELL:
        price = getattr(a, which)
        had = (links.get(kind) or {}).get("by_users") or {}
        print(f"{kind}: {most} link{'s' if most > 1 else ''} at {money(price)} each ...", flush=True)
        rungs, made, kept = a_ladder(square, kind, name, price, most, a.site, note, location, had)
        links[kind] = {"item": name, "price": price, "most": most, "by_users": rungs}
        block[kind] = rungs
        print(f"  {made} made, {kept} already there. One user: {rungs['1']['url']}")
    links["location"] = {"id": location["id"], "name": location.get("name")}
    folder.save_links(links)

    lines = {
        "buy_office_url": block["seats"]["1"]["url"],
        "buy_renewal_url": block["renewal"]["1"]["url"],
        "buy_seats_max": most,
        "buy_office_links": {n: r["url"] for n, r in sorted(block["seats"].items(), key=lambda kv: int(kv[0]))},
        "buy_renewal_links": {n: r["url"] for n, r in sorted(block["renewal"].items(), key=lambda kv: int(kv[0]))},
    }
    out = os.path.join(a.folder, "square-site.json")
    write_json(out, lines)
    print(f"\nThe website's settings are in {out}.")
    print("Copy those five entries into ExcaliburSite\\site.json, run python build.py, and push.")
    print(f'\n  "buy_office_url": "{lines["buy_office_url"]}",')
    print(f'  "buy_renewal_url": "{lines["buy_renewal_url"]}",')
    print(f'  "buy_seats_max": {most},')
    print(f'  "buy_office_links": {{ ... {most} links ... }},')
    print(f'  "buy_renewal_links": {{ ... {most} links ... }}')


# ---- reading what was bought -----------------------------------------------

def money(cents):
    return f"${cents / 100:,.2f}"


def what_was_bought(order, links):
    """(kind, users) from an order's line items: how many seats, or how many
    users' worth of updates. `None` when it is not one of ours."""
    kind, users = None, 0
    for item in (order.get("line_items") or []):
        name = (item.get("name") or "").lower()
        count = int(float(item.get("quantity") or 1))
        if "renewal" in name or "updates" in name:
            kind, users = "renewal", users + count
        elif "office" in name or "per user" in name or "seat" in name:
            kind, users = kind or "seats", users + count
    return kind, users


def answers_in(order):
    """Whatever the buyer typed at checkout, wherever Square puts it.

    Square has moved these about between versions, so rather than trusting
    one place this walks the order for anything that looks like a question
    and an answer. What it cannot find, it does not invent.
    """
    found = {}

    def walk(thing):
        if isinstance(thing, dict):
            title = thing.get("title") or thing.get("label") or thing.get("name")
            value = thing.get("text") or thing.get("value") or thing.get("answer")
            if isinstance(title, str) and isinstance(value, str) and value.strip():
                found.setdefault(title.strip().lower(), value.strip())
            for v in thing.values():
                walk(v)
        elif isinstance(thing, list):
            for v in thing:
                walk(v)

    walk(order.get("metadata") or {})
    for key in ("custom_fields", "fulfillments", "line_items", "source", "checkout_options"):
        walk(order.get(key))
    return found


def one_of(answers, *words):
    """The answer to a question whose title has all of these words in it."""
    for title, value in answers.items():
        if all(w in title for w in words):
            return value
    return None


def company_of(order, payment, customer):
    """The company a license is for, or None. Never guessed from an email."""
    answers = answers_in(order)
    company = one_of(answers, "company")
    if company:
        return company.strip()
    if customer and (customer.get("company_name") or "").strip():
        return customer["company_name"].strip()
    note = (payment.get("note") or "").strip()
    if note.lower().startswith("company:"):
        return note.split(":", 1)[1].strip()
    return None


def email_of(order, payment, customer):
    answers = answers_in(order)
    for words in (("administrator",), ("email",)):
        found = one_of(answers, *words)
        if found and "@" in found:
            return found.strip()
    for place in (payment.get("buyer_email_address"), (customer or {}).get("email_address")):
        if place and "@" in place:
            return place.strip()
    return None


def paid_orders(square, folder, days, include_issued=False):
    """Completed payments in the window, each with its order and buyer."""
    since = (datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(days=days)).strftime("%Y-%m-%dT%H:%M:%SZ")
    payments = square.paged("/v2/payments", "payments", {"begin_time": since, "sort_order": "ASC", "limit": 100})
    issued = folder.issued()
    links = folder.links()
    out = []
    orders = {}
    customers = {}
    for payment in payments:
        if payment.get("status") != "COMPLETED":
            continue
        if payment["id"] in issued and not include_issued:
            continue
        order = {}
        if payment.get("order_id"):
            if payment["order_id"] not in orders:
                orders[payment["order_id"]] = (square.call("GET", f"/v2/orders/{payment['order_id']}") or {}).get("order") or {}
            order = orders[payment["order_id"]]
        kind, users = what_was_bought(order, links)
        if kind is None:
            continue  # somebody's shop sale, not ours
        customer = None
        if payment.get("customer_id"):
            if payment["customer_id"] not in customers:
                try:
                    customers[payment["customer_id"]] = (square.call("GET", f"/v2/customers/{payment['customer_id']}") or {}).get("customer") or {}
                except SquareSaid:
                    customers[payment["customer_id"]] = {}
            customer = customers[payment["customer_id"]]
        out.append({
            "payment": payment,
            "order": order,
            "kind": kind,
            "users": users,
            "company": company_of(order, payment, customer),
            "email": email_of(order, payment, customer),
            "when": payment.get("created_at", ""),
            "paid": (payment.get("amount_money") or {}).get("amount", 0),
            "issued": issued.get(payment["id"]),
        })
    return out


# ---- the licenses this office already has ----------------------------------

def licenses_here(folder):
    """Every license in the folder, newest first, so a renewal or extra seats
    build on the one the customer already has."""
    out = []
    if not os.path.isdir(folder.licenses):
        return out
    for name in sorted(os.listdir(folder.licenses)):
        if not name.lower().endswith("." + publish.PRODUCTS["view"]["extension"]):
            continue
        try:
            lic = json.loads(open(os.path.join(folder.licenses, name), encoding="utf-8-sig").read())
        except Exception:
            continue
        if isinstance(lic, dict) and lic.get("company"):
            lic["_file"] = os.path.join(folder.licenses, name)
            out.append(lic)
    out.sort(key=lambda l: (l.get("issued", ""), l.get("_file", "")), reverse=True)
    return out


def held_by(folder, company):
    same = simple(company)
    for lic in licenses_here(folder):
        if simple(lic["company"]) == same:
            return lic
    return None


def simple(name):
    """A company name with the noise taken out, for matching one sale to the
    licence a customer already has: "Acme Builders, LLC" and "Acme Builders"."""
    name = re.sub(r"[^a-z0-9 ]", " ", (name or "").lower())
    words = [w for w in name.split() if w not in {"inc", "llc", "ltd", "co", "corp", "company", "the"}]
    return " ".join(words)


def next_license(folder, sale, company, key_path):
    """What this sale means for that company's license: a first one, more
    users on the one they have, or another year of updates."""
    today = datetime.date.today()
    held = held_by(folder, company)
    if held and held.get("updates_through") == publish.FOREVER:
        return None, "that company's license already covers every user and every update, for good. Nothing to issue."
    if sale["kind"] == "seats":
        if held:
            if held.get("users") == 0:
                return None, "that company's license already covers every user. Nothing to issue."
            users = held["users"] + sale["users"]
            through = held["updates_through"]
            note = "Added users"
        else:
            users, through, note = sale["users"], publish.a_year_on(today), ""
    else:
        if not held:
            return None, ("a renewal for a company with no license here. Issue their first license first, "
                          "or name the company with --company.")
        users = held["users"]
        # Another year, from the day their updates run out - or from today,
        # when they renew late. Never a year they already had.
        from_day = today
        if held["updates_through"] > today.isoformat():
            from_day = datetime.date.fromisoformat(held["updates_through"])
        through = publish.a_year_on(from_day)
        note = "Updates renewed"
    lic = publish.make_license(
        key_path, company, users=users, updates_through=through, note=note,
        id=held["id"] if held else None,
    )
    return lic, None


# ---- issuing ---------------------------------------------------------------

def email_text(lic, sale, site):
    users = "every user" if lic["users"] == 0 else f"{lic['users']} user" + ("" if lic["users"] == 1 else "s")
    through = "for good" if lic["updates_through"] == publish.FOREVER else f"through {publish.long_date(lic['updates_through'])}"
    what = "Your Excalibur View Office license" if sale["kind"] == "seats" else "Your renewed Excalibur View Office license"
    return (
        f"Subject: {what} - {lic['company']}\n"
        f"To: {sale.get('email') or '(the administrator)'}\n"
        "\n"
        f"Thanks for buying Excalibur View Office. The license file is attached: {users}, updates {through}.\n"
        "\n"
        "One person does this once, on the computer running the office server. Everybody\n"
        "else gets it by connecting - nobody else ever sees a license.\n"
        "\n"
        "  1. Open Excalibur View there and sign in as an administrator.\n"
        "  2. Studio (on the left) - Office - License.\n"
        "  3. Either Add license file... and pick the attachment, or Paste license... and\n"
        "     paste the block below. Either way it is checked on your own server and\n"
        "     nothing is sent anywhere.\n"
        "\n"
        "The license, if you would rather paste than go looking for the attachment:\n"
        "\n"
        f"{json.dumps(lic, indent=2, sort_keys=True)}\n"
        "\n"
        "Keep the file somewhere safe. If the server is ever rebuilt, add it again.\n"
        "\n"
        f"Anything at all, reply to this email.\n"
        "\n"
        "Creede Guardamondo\n"
        f"Excalibur Construction Technologies · {site}\n"
    )


def issue_one(folder, sale, company, quiet=False, site="https://excaliburct.com"):
    lic, why = next_license(folder, sale, company, folder.key)
    if why:
        folder.say(f"  {sale['payment']['id']}: {why}", quiet)
        return None
    os.makedirs(folder.licenses, exist_ok=True)
    path = os.path.join(folder.licenses, publish.license_file_name(lic))
    publish.write_license(lic, path)
    letter = os.path.splitext(path)[0] + " - email.txt"
    with open(letter, "w", encoding="utf-8") as f:
        f.write(email_text(lic, sale, site))
    folder.remember(sale["payment"]["id"], {
        "company": lic["company"],
        "license": lic["id"],
        "users": lic["users"],
        "updates_through": lic["updates_through"],
        "file": path,
        "kind": sale["kind"],
        "paid": sale["paid"],
        "when": sale["when"],
        "issued": datetime.datetime.now().isoformat(timespec="seconds"),
    })
    users = "every user" if lic["users"] == 0 else f"{lic['users']} users"
    folder.say(f"  {lic['company']}: {users}, updates through {lic['updates_through']} -> {os.path.basename(path)}", quiet)
    return path


def cmd_orders(a):
    folder = Folder(a.folder)
    sales = paid_orders(folder.square(a.sandbox), folder, a.days, include_issued=True)
    if not sales:
        print(f"No Excalibur orders in the last {a.days} days.")
        return
    for sale in sales:
        mark = "issued " if sale["issued"] else "WAITING"
        who = sale["company"] or "(no company name)"
        print(f"{mark}  {sale['when'][:10]}  {money(sale['paid']):>10}  {sale['kind']:<7} "
              f"{sale['users']:>3} user(s)  {who}  {sale['email'] or ''}")
        if not sale["issued"] and not sale["company"]:
            print(f"           say who it is:  python square.py issue --payment {sale['payment']['id']} "
                  f"--company \"Their Company\"")


def cmd_issue(a):
    folder = Folder(a.folder)
    sales = paid_orders(folder.square(a.sandbox), folder, a.days)
    if a.payment:
        sales = [s for s in sales if s["payment"]["id"] == a.payment]
        if not sales:
            raise SystemExit(f"{a.payment} is not a new Excalibur order in the last {a.days} days. "
                             "`orders` lists what there is.")
    if not sales:
        folder.say("Nothing new to issue.", a.quiet)
        return
    folder.say(f"{len(sales)} order(s) to issue:", a.quiet)
    waiting = 0
    for sale in sales:
        company = a.company or sale["company"]
        if not company:
            waiting += 1
            folder.say(f"  {sale['payment']['id']} ({money(sale['paid'])}, {sale['when'][:10]}): waiting - "
                       f"Square did not pass on a company name. Run: python square.py issue "
                       f"--payment {sale['payment']['id']} --company \"Their Company\"", a.quiet)
            continue
        if a.dry_run:
            folder.say(f"  would issue: {company}, {sale['kind']}, {sale['users']} user(s)", a.quiet)
            continue
        issue_one(folder, sale, company, a.quiet, a.site)
    if waiting:
        folder.say(f"{waiting} order(s) waiting on a company name.", a.quiet)
    if not a.quiet and not a.dry_run:
        print(f"\nThe files and their emails are in {folder.licenses}. Attach the .evlicense to the email.")


def cmd_watch(a):
    a.quiet = True
    a.company = None
    a.payment = None
    a.dry_run = False
    cmd_issue(a)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--folder", default=os.path.dirname(os.path.abspath(__file__)),
                   help="the signing folder (where the key and the token are)")
    p.add_argument("--sandbox", action="store_true", help="Square's test account rather than the real one")
    p.add_argument("--site", default="https://excaliburct.com")
    sub = p.add_subparsers(dest="command", required=True)
    s = sub.add_parser("setup", help="make the payment links the website's Buy buttons use")
    s.add_argument("--price", type=int, default=SEATS_PRICE, help="cents per user, one time")
    s.add_argument("--renewal", type=int, default=RENEWAL_PRICE, help="cents per user per year")
    s.add_argument("--most", type=int, default=MOST_SEATS,
                   help="the largest number of users with a link of its own (default %(default)s)")
    s.add_argument("--location", help="which Square location, by name or id")
    o = sub.add_parser("orders", help="what has been paid, and what is issued")
    o.add_argument("--days", type=int, default=60)
    i = sub.add_parser("issue", help="sign a license for each new paid order")
    i.add_argument("--days", type=int, default=60)
    i.add_argument("--payment", help="just this one payment")
    i.add_argument("--company", help="the company name for it, when Square did not pass one on")
    i.add_argument("--dry-run", action="store_true")
    i.add_argument("--quiet", action="store_true")
    w = sub.add_parser("watch", help="issue quietly, for a scheduled task")
    w.add_argument("--days", type=int, default=7)
    a = p.parse_args()
    try:
        {"setup": cmd_setup, "orders": cmd_orders, "issue": cmd_issue, "watch": cmd_watch}[a.command](a)
    except SquareSaid as e:
        raise SystemExit(str(e))


if __name__ == "__main__":
    main()
