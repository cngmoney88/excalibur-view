# Reaching the office server from a jobsite

On the shop network a seat finds its server by broadcast. From a truck it
cannot, and the answer today is a Cloudflare tunnel somebody sets up by hand.
This is what replaces that: press a button, get an address, nothing opened on
the firewall.

## The decision this rests on

**Each shop uses its own Cloudflare account.** Decided 23 September 2026.

Not ours. The product sells data sovereignty — your drawings, your server,
your building — and routing every customer's remote access through a tunnel on
our domain, in our account, would make that untrue in the one place it matters,
which is the path the drawings actually travel. It would also make us a single
point of failure for other companies' work, unpriced and forever.

Hosting on our domain stays available as a paid add-on with a stated price. It
is then a product with a margin rather than a liability absorbed by accident.

## The thing that has to be built first

**The server has no rate limiting. None.** Not on sign-in, not on the join
code, not anywhere. `grep` the crate and there is nothing.

On a shop network that is defensible: to guess at anything you have to already
be in the building. On the internet it is not, and the join code is the reason.
It is a single server-wide secret that names nobody, never expires, is not used
up, and gets whoever types it an account on the server. Unlimited guesses from
anywhere against a secret that never rotates is not a tunnel problem that can
be fixed later; it is the tunnel turning an acceptable weakness into a hole.

So remote access ships with, and not before:

- **A limit on sign-in attempts**, by account and by address, with a delay that
  grows. The audit log already records a sign-in refused for an address nobody
  has, which is the case an administrator most wants to see.
- **A limit on join attempts**, by address, harder than the sign-in one —
  nobody legitimately types a join code twenty times.
- **A join code that can expire**, and an administrator who is told plainly
  that an unexpiring code and a public address are a poor pair.

None of this is interesting work. All of it is the difference between a feature
and an incident.

## How it works

A Cloudflare tunnel is an outbound connection from the shop's server to
Cloudflare, which then answers on a public hostname. Nothing is opened on the
shop's firewall: the connection is made from the inside, the way a browser
makes one.

### What the shop does, once

1. A free Cloudflare account.
2. Zero Trust → Networks → Tunnels → Create a tunnel.
3. Point it at `http://localhost:8714` and pick a hostname.
4. Copy the token it gives them.

### What they do in Excalibur View, once

**Studio → Office → Remote access.** Paste the token, press Turn on. That is
the whole of it. The server keeps the token the way it keeps anything else
that matters, starts the connector, and shows the address.

The $1,500 configuration service does all of the above for a shop that would
rather it were done for them.

### What the server does

- **Fetches `cloudflared`** the first time it is asked, checked against a
  pinned SHA-256, the way the PDF engine already is. Kept beside the server.
  A build that cannot verify the digest does not run the file.
- **Runs it as a child process** and restarts it if it stops, the same way it
  looks after itself now.
- **Reports the address** to the Studio panel, and to seats, which is what
  makes the next part work.
- **A sealed server never does any of this.** Every outward connection is
  refused, and that is the whole point of the edition. The panel says so
  rather than showing a button that cannot work.

### What a seat does

A seat that has signed in to a server on the shop network already remembers
where it was. It now also remembers the public address the server told it
about, and when the broadcast finds nothing — which is what being in a truck
looks like — it tries that instead.

So the estimator who used it in the office opens it on site and it works. They
never type an address. That is the whole feature, and everything above is what
it costs.

## What this is not

**Not a VPN.** One service is reachable, on one hostname: this server's API.
Nothing else on the shop's network is exposed, because nothing else is behind
the tunnel.

**Not a way in for us.** The tunnel is in the shop's Cloudflare account, on
their hostname, made with their token. We cannot reach it, cannot see it, and
cannot take it away. If they stop paying us, it keeps working; if they leave
us, nothing of theirs is ours to hold.

**Not on by default.** A shop that does not want it never presses the button,
and a shop that changes its mind presses it again. The token can be revoked
from their Cloudflare dashboard without asking anybody.

## Build order

1. ~~Rate limiting on sign-in and joining, and a join code that can expire.~~
   Done, except the expiring code.
2. ~~The connector: fetch, verify, run, restart, report.~~ Done.
3. ~~The Office panel: token in, address out, and a sealed server saying why
   not.~~ Done.
4. ~~A seat that falls back to the public address when the broadcast finds
   nothing.~~ Done.

What is left of item one: **the join code still never expires.** An unexpiring
server-wide secret was defensible when the only way to type it was to be
standing in the building. It is the weakest thing about a server with a public
address, and it is the next thing to do here.

Nobody has run this against a real Cloudflare tunnel yet. Everything above is
built and tested, including the parts that refuse — a digest that does not
match, a sealed server, a hostname with `https://` typed into it — but a shop's
own tunnel has never been turned on end to end. That is the first thing to do
with a real account, and it is worth doing before it is sold to anybody.
