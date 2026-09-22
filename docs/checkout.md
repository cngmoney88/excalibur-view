# Selling Office through Square

Square takes the money. Licenses are signed on the publisher's own PC, beside
the key, and emailed by hand. Nothing about a customer, and nothing that can
make a license, is ever on a server.

```
  the website            Square                 this PC
  Buy Office   ----->    checkout    ----->     square.py issue  ----->  email
  (payment link)         (card)                 signs .evlicense         with the file
```

`tools/square.py` does both ends: it makes the payment links the website's Buy
buttons point at, and it turns paid orders into signed licenses.
`tools/square_check.py` runs the whole thing against a stand-in Square, so the
flow can be changed without waiting for a real sale.

## One-time setup

1. **A Square app.** In Square's Developer Console, make an application for
   Excalibur View and copy its **production** access token.
2. **Keep the token beside the key**, in `ExcaliburSigning\square-token.txt` —
   the token on one line, nothing else. It never leaves that PC, the same way
   the GitHub token never does. Permissions it needs: `PAYMENTS_READ`,
   `ORDERS_READ`, `ORDERS_WRITE`, `PAYMENTS_WRITE`, `CUSTOMERS_READ`,
   `MERCHANT_PROFILE_READ`.
3. **Make the payment links:**

   ```
   cd C:\Users\creed\dev\ExcaliburSigning
   python square.py setup
   ```

   Square's hosted checkout has no quantity box — the number of users is fixed
   when a link is made — so this makes one link per user count: Office at $250
   a user for 1 through 25 users, and the same ladder for the $100 renewal.
   Every link asks the buyer for the company name and the administrator's
   email, and sends them to excaliburct.com/thanks/ afterwards. Prices come
   from `--price` and `--renewal`, in cents; `--most` sets the top of the
   ladder.

   Running it again makes nothing new. It reads `square-links.json`, sees what
   it already made, and only fills gaps — so raising `--most` from 25 to 40
   costs fifteen links, not fifty-five.
4. **The website:** `setup` writes `square-site.json`. Copy its five entries
   into `ExcaliburSite\site.json`, run `python build.py`, and push. The
   pricing page grows a **How many users?** stepper that works out the total
   and points Buy at the right link. Until the links are set the stepper stays
   hidden and Buy emails `hello@excaliburct.com`, so the site is never broken.
   Past the top of the ladder the page offers an invoice instead.

## When somebody buys

```
python square.py orders      what has been paid, and what is already issued
python square.py issue       sign a license for everything new
```

`issue` works out from the order what it is:

- **A first purchase** — a license for the seats bought, updates for a year.
- **More seats for a company that already has one** — the same license id, the
  same updates date, the new total of users.
- **A renewal** — the same id and users, updates pushed out a year from the day
  they ran out (or from today, if they renewed late).

Each one writes two files into `ExcaliburSigning\licenses`: the `.evlicense`
and `<Company> - email.txt`, the email to send with it. Attach the license to
that email and send it. That is the only manual step, and it is deliberate: the
file goes out under your name, from your mailbox.

**A company name is never guessed.** If Square does not pass the answer on with
the order, that order waits and says so:

```
python square.py issue --payment <payment id> --company "Their Company"
```

## Hourly, without you

Task Scheduler → Create Task → run every hour:

```
Program:    py
Arguments:  -3 square.py watch
Start in:   C:\Users\creed\dev\ExcaliburSigning
```

`watch` prints nothing and writes what it did to `square-log.txt`. New orders
turn into signed licenses in the `licenses` folder within the hour, waiting for
you to send them.

## What it will not do

- **Nothing is charged automatically.** A renewal is a payment somebody makes,
  not a card on file we keep charging.
- **Nothing is issued twice.** `square-orders.json` remembers every payment that
  has a license; running `issue` again does nothing.
- **A refund does not take a license back.** A license file, once sent, works
  for good — that is the promise on the website. If a sale is refunded, take
  the file out of the folder and keep the note; nothing else to do.
- **Nothing about a customer leaves the PC.** Square holds the payment; the
  folder holds the license. There is no server in between and nothing to breach.

## When something looks wrong

| What you see | What it means |
|---|---|
| `square-token.txt is not there` | Step 2 above. |
| `Square said 401` | The token is wrong, or it is a sandbox token against the live account. |
| `Square said 403` | The token's app is missing one of the permissions in step 2. |
| `waiting - Square did not pass on a company name` | Run `issue --payment … --company "…"`. |
| `a renewal for a company with no license here` | Their first license was issued somewhere else, or under a different spelling. Name it with `--company`, spelled as their existing license is. |
| Nothing at all in `orders` | The buyer paid through something other than these links, or it is older than `--days` (60 by default). |
| The website's stepper is not there | `site.json` has no `buy_office_links` yet, or `buy_seats_max` is 0. Run `setup`, copy `square-site.json` across, rebuild. |
| Somebody wants more users than the ladder goes to | `python square.py setup --most 40`, then copy `square-site.json` across again. |

`python tools/square_check.py` runs the whole flow — the ladder of links and
what a second run does to it, a first purchase, added seats, a renewal, an
order with no company name, and a sale that is not ours — against a stand-in
Square with a throwaway key. It touches neither the real account nor the real
key.
