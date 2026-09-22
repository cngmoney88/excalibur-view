#!/usr/bin/env python3
"""Checks square.py end to end against a stand-in Square.

Nothing here touches the real Square or the real key: a little HTTP server
answers the way Square does, and the licenses are signed with a throwaway
key. Run it from anywhere:  python3 tools/square_check.py
"""

import json, threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import urlparse, parse_qs

STATE = {"links": [], "payments": [], "orders": {}, "customers": {}}

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def send_json(self, code, body):
        raw = json.dumps(body).encode()
        self.send_response(code); self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw))); self.end_headers(); self.wfile.write(raw)
    def do_GET(self):
        if self.headers.get("Authorization") != "Bearer test-token":
            return self.send_json(401, {"errors": [{"detail": "This request could not be authorized."}]})
        u = urlparse(self.path)
        if u.path == "/v2/locations":
            return self.send_json(200, {"locations": [{"id": "L1", "name": "Mesa Fab", "status": "ACTIVE"}]})
        if u.path == "/v2/payments":
            q = parse_qs(u.query)
            page = STATE["payments"]
            cursor = q.get("cursor", [None])[0]
            if cursor is None:
                return self.send_json(200, {"payments": page[:2], "cursor": "more" if len(page) > 2 else None})
            return self.send_json(200, {"payments": page[2:]})
        if u.path.startswith("/v2/orders/"):
            oid = u.path.rsplit("/", 1)[1]
            return self.send_json(200, {"order": STATE["orders"].get(oid, {})})
        if u.path.startswith("/v2/customers/"):
            cid = u.path.rsplit("/", 1)[1]
            return self.send_json(200, {"customer": STATE["customers"].get(cid, {})})
        return self.send_json(404, {"errors": [{"detail": "no such endpoint " + u.path}]})
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])) or b"{}")
        if urlparse(self.path).path == "/v2/online-checkout/payment-links":
            n = len(STATE["links"]) + 1
            link = {"id": f"LINK{n}", "version": 1, "url": f"https://square.link/u/test{n}",
                    "long_url": f"https://checkout.square.site/test{n}", "order_id": f"ORD{n}"}
            STATE["links"].append({"link": link, "body": body})
            return self.send_json(200, {"payment_link": link})
        return self.send_json(404, {"errors": [{"detail": "no such endpoint"}]})

def serve():
    server = HTTPServer(("127.0.0.1", 0), H)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, f"http://127.0.0.1:{server.server_port}"


import json, os, shutil, subprocess, sys, datetime
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import publish, square as sq

HERE = os.path.dirname(os.path.abspath(__file__))
TOOLS = HERE
FOLDER = os.path.join(os.environ.get("TMPDIR", "/tmp"), "square-check-signing")
shutil.rmtree(FOLDER, ignore_errors=True); os.makedirs(FOLDER)
open(os.path.join(FOLDER, "hyperview-signing.key"), "w").write("test-2026\n" + "09" * 32 + "\n")
# Saved the way Notepad saves it: the name typed as square-token.txt, and
# .txt put on the end again. This is what it looks like on a real PC.
open(os.path.join(FOLDER, "square-token.txt.txt"), "w").write("test-token\n")
server, base = serve()
sq.LIVE = base
results = []
def chk(name, ok, extra=""):
    results.append(bool(ok))
    print(("PASS " if ok else "FAIL ") + name + ("" if ok else "\n     -- " + str(extra)[:900]))
    sys.stdout.flush()

def run(*args):
    out = subprocess.run([sys.executable, "-c",
        "import sys;sys.path.insert(0, %r);import square;square.LIVE=%r;square.main()" % (TOOLS, base)]
        + ["--folder", FOLDER] + list(args), capture_output=True, text=True)
    return out.returncode, out.stdout + out.stderr

def payment(pid, cents, order_id, when, email=None, customer=None):
    p = {"id": pid, "status": "COMPLETED", "created_at": when, "order_id": order_id,
         "amount_money": {"amount": cents, "currency": "USD"}}
    if email: p["buyer_email_address"] = email
    if customer: p["customer_id"] = customer
    STATE["payments"].append(p)

def order(oid, item, qty, company=None, email=None):
    o = {"id": oid, "line_items": [{"name": item, "quantity": str(qty),
         "base_price_money": {"amount": 25000, "currency": "USD"}}]}
    if company:
        o["metadata"] = {}
        o["fulfillments"] = [{"custom_fields": [
            {"title": sq.COMPANY_FIELD, "text": company},
            {"title": sq.EMAIL_FIELD, "text": email or ""}]}]
    STATE["orders"][oid] = o

today = datetime.date.today()
now = datetime.datetime.now(datetime.timezone.utc)
stamp = lambda days: (now - datetime.timedelta(days=days)).strftime("%Y-%m-%dT%H:%M:%SZ")

# 1. setup - a link per user count, because Square's checkout has no stepper
MOST = 6
code, out = run("setup", "--most", str(MOST))
chk("the token is found however Windows spelled the file name", "is not there" not in out, out)
links = json.load(open(os.path.join(FOLDER, "square-links.json")))
rungs = links["seats"]["by_users"]
body = STATE["links"][0]["body"]
chk("setup makes a link for every user count, for both products",
    code == 0 and len(rungs) == MOST and len(links["renewal"]["by_users"]) == MOST, (code, out))
chk("each link is fixed to its own number of users",
    all(int(STATE["links"][i]["body"]["order"]["line_items"][0]["quantity"]) == i + 1 for i in range(MOST)),
    [l["body"]["order"]["line_items"][0]["quantity"] for l in STATE["links"]])
chk("every link sells the same item, so issue can still read it",
    {l["body"]["order"]["line_items"][0]["name"] for l in STATE["links"]} == {sq.SEATS_ITEM, sq.RENEWAL_ITEM},
    [l["body"]["order"]["line_items"][0]["name"] for l in STATE["links"]])
chk("the checkout asks for the company and the administrator's email",
    [f["title"] for f in body["checkout_options"]["custom_fields"]] == [sq.COMPANY_FIELD, sq.EMAIL_FIELD], body)
chk("and sends the buyer to the thank-you page",
    body["checkout_options"]["redirect_url"] == "https://excaliburct.com/thanks/", body)
chk("nothing random goes to Square, so making them twice asks for the same links",
    len({l["body"]["idempotency_key"] for l in STATE["links"]}) == MOST * 2, "keys repeat")

made_so_far = len(STATE["links"])
code, out = run("setup", "--most", str(MOST))
chk("running setup again makes no new links", len(STATE["links"]) == made_so_far and "0 made" in out, out)

code, out = run("setup", "--most", str(MOST + 2))
chk("asking for more users only makes the missing ones",
    len(STATE["links"]) == made_so_far + 4 and "2 made" in out, out)

site = json.load(open(os.path.join(FOLDER, "square-site.json")))
chk("setup writes the website's settings out",
    site["buy_seats_max"] == MOST + 2
    and len(site["buy_office_links"]) == MOST + 2
    and site["buy_office_url"] == site["buy_office_links"]["1"], site)
chk("setup prints the lines for the website", '"buy_office_url"' in out and '"buy_seats_max"' in out, out)

# 2. a first purchase of six seats
payment("P1", 150000, "O1", stamp(5), email="pat@acme.test")
order("O1", sq.SEATS_ITEM, 6, "Acme Builders, LLC", "pat@acme.test")
code, out = run("issue")
lic = json.load(open(os.path.join(FOLDER, "licenses", "Acme Builders, LLC.evlicense")))
chk("a first purchase becomes a license for the seats bought",
    lic["users"] == 6 and lic["company"] == "Acme Builders, LLC" and lic["edition"] == "office", (out, lic))
chk("with a year of updates", lic["updates_through"] == publish.a_year_on(today), lic)
letter = open(os.path.join(FOLDER, "licenses", "Acme Builders, LLC - email.txt")).read()
chk("and an email ready to send", "pat@acme.test" in letter and "Add license file" in letter, letter[:300])

# 3. the same order again changes nothing
code, out = run("issue")
chk("issuing twice never makes a second license", "Nothing new" in out, out)

# 4. two more seats later
payment("P2", 50000, "O2", stamp(3), email="pat@acme.test")
order("O2", sq.SEATS_ITEM, 2, "Acme Builders, LLC", "pat@acme.test")
code, out = run("issue")
after = json.load(open(os.path.join(FOLDER, "licenses", "Acme Builders, LLC.evlicense")))
chk("more seats replace the license with the new total", after["users"] == 8, (out, after))
chk("under the same license id", after["id"] == lic["id"], (lic["id"], after["id"]))
chk("and the same updates date", after["updates_through"] == lic["updates_through"], after)

# 5. a renewal
payment("P3", 80000, "O3", stamp(1), email="pat@acme.test")
order("O3", sq.RENEWAL_ITEM, 8, "Acme Builders, LLC", "pat@acme.test")
code, out = run("issue")
renewed = json.load(open(os.path.join(FOLDER, "licenses", "Acme Builders, LLC.evlicense")))
chk("a renewal pushes the updates date out a year",
    renewed["updates_through"] == publish.a_year_on(datetime.date.fromisoformat(after["updates_through"])), renewed)
chk("keeping the users and the id", renewed["users"] == 8 and renewed["id"] == lic["id"], renewed)

# 6. an order Square gave no company name for
payment("P4", 75000, "O4", stamp(1), email="creede@hudson.test")
order("O4", sq.SEATS_ITEM, 3)
code, out = run("issue")
chk("an order with no company name waits rather than guessing", "waiting" in out and "--company" in out, out)
chk("and nothing was written for it", not os.path.exists(os.path.join(FOLDER, "licenses", "Hudson Asphalt.evlicense")), out)
code, out = run("issue", "--payment", "P4", "--company", "Hudson Asphalt")
hud = json.load(open(os.path.join(FOLDER, "licenses", "Hudson Asphalt.evlicense")))
chk("naming the company finishes it", hud["users"] == 3 and hud["id"] != lic["id"], (out, hud))

# 7. somebody else's sale on the same Square account
payment("P5", 4200, "O5", stamp(1))
order("O5", "Shop hours", 1, "Somebody Else")
code, out = run("issue")
chk("a sale that is not ours is left alone", "Nothing new" in out, out)

# 8. every license signed checks out against the key
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
key = Ed25519PrivateKey.from_private_bytes(bytes.fromhex("09" * 32))
every = True
for name in os.listdir(os.path.join(FOLDER, "licenses")):
    if not name.endswith(".evlicense"): continue
    l = json.load(open(os.path.join(FOLDER, "licenses", name)))
    try:
        key.public_key().verify(bytes.fromhex(l["signature"]), publish.license_payload(l))
    except Exception as e:
        every = False; print("   ", name, e)
chk("every license file checks out against the key", every)

# 9. the list of orders reads plainly, and watch is quiet
code, out = run("orders")
chk("orders lists what is issued and what is waiting", "issued" in out and "Acme Builders, LLC" in out, out)
code, out = run("watch")
chk("watch says nothing when there is nothing to do", code == 0 and out.strip() == "", repr(out))
log = open(os.path.join(FOLDER, "square-log.txt")).read()
chk("but writes it down", "Nothing new to issue" in log, log[-300:])

print(f"\n{sum(results)}/{len(results)} passed")
sys.exit(0 if all(results) else 1)
