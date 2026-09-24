# Getting a license back, without asking anybody

Somebody loses the email with their `.evlicense` in it. Today that means
reissuing it by hand, which means they wait on Creede's inbox — over a weekend,
that is Monday, and a shop that cannot start its server on Monday morning is a
shop looking at Bluebeam again.

This is how they get it back themselves, with no account, no password, and
nothing running anywhere.

## What the customer has

Not the license id — that was in the file they lost. What they still have is the
**Square receipt**, in their own inbox, with a payment id on it. It is long,
random, and theirs.

## How it works

`square.py` already publishes every license under `SHA-256(id)`, which is how a
server fetches its own renewal. It now publishes a second copy under
`SHA-256("receipt:" + payment id)`.

A browser can work that out. So the page needs no server at all:

1. Customer pastes their payment id.
2. The page hashes it, in the browser.
3. The page fetches `/f/<hash>.evlicense`.
4. It downloads, or it says it could not find one.

Nothing is looked up, so there is nothing to look up *with*: somebody who has
not got a receipt has nothing to type. The customer list stays unwalkable,
because the addresses are hashes of things only the buyers hold.

## The page

For the site. Plain, no framework, and it must sit on a page that says what to
do when it does not work.

```html
<label for="receipt">The payment ID from your Square receipt</label>
<input id="receipt" autocomplete="off" spellcheck="false">
<button id="fetch">Get my license</button>
<p id="said" role="status"></p>

<script>
document.getElementById("fetch").addEventListener("click", async () => {
  const said = document.getElementById("said");
  const typed = document.getElementById("receipt").value.trim();
  if (!typed) { said.textContent = "Paste the payment ID from your receipt."; return; }
  said.textContent = "Looking…";
  const bytes = new TextEncoder().encode("receipt:" + typed);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  const name = [...new Uint8Array(digest)].map(b => b.toString(16).padStart(2, "0")).join("");
  const response = await fetch(`/f/${name}.evlicense`, { cache: "no-store" });
  if (!response.ok) {
    said.textContent = "No license at that ID. Check you copied the whole thing, "
      + "or email hello@excaliburct.com and we will sort it out.";
    return;
  }
  const blob = await response.blob();
  const link = document.createElement("a");
  link.href = URL.createObjectURL(blob);
  link.download = "excalibur-view.evlicense";
  link.click();
  URL.revokeObjectURL(link.href);
  said.textContent = "Downloaded. Double-click it, or drop it on the License screen.";
});
</script>
```

`crypto.subtle` needs HTTPS, which the site is.

## What it does not do

It does not prove who is asking. Anybody holding that payment id can fetch that
license — which is the same as anybody holding the email it was sent in. A
license is not a secret worth more than the receipt that bought it, and the
alternative is an account system, which is a service to run, a password to
reset, and a seat phoning home. That trade is written down in the punch list
and this is the cheap half of it.
