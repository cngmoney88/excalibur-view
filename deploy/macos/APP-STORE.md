# The Mac App Store, when you want it

Developer ID ships first and is the main channel. This is what the App Store
would cost on top, written now so the decision is made with the real list
rather than a guess.

Nothing here is wasted work if you never do it.

## What has to change in the program

**1. Self-update has to go.** App Store apps update through the App Store,
full stop. `install::put_in_place`, `hub::update::swap_in` and the whole
signed-release path must be compiled out of that build — not disabled at
runtime, compiled out, or review will fail it.

That is the real cost. Today a fix reaches every seat the hour you publish it.
In the App Store it reaches them when Apple says so.

**2. The sandbox.** `entitlements-appstore.plist` beside this file asks for
what the program needs: user-selected files, app-scoped bookmarks, network
client and server, printing, JIT for Metal. Expect two things to be awkward:

- **Finding the office server on the local network.** The sandbox permits it,
  but macOS shows a local-network prompt the first time. Fine, just different.
- **The profiles folder beside the program.** A sandboxed app cannot write
  next to itself. Tool chests and profiles have to move to the app's container
  under `~/Library/Containers/`. That is a code change, not a setting.

**3. "Host the drawings here" is the open question.** A sandboxed app running
a server other machines connect to is allowed, but it is the kind of thing a
reviewer asks about. It may need to be absent from the App Store build, which
makes that build the free single-person app and nothing more — which is
probably the right shape for it anyway.

## What has to change outside the program

**A separate certificate.** "Apple Distribution" and "Mac Installer
Distribution", not the Developer ID one. Both from the same account.

**An App Store Connect listing.** Screenshots, description, category,
support URL, privacy details. The privacy section is the easy one for you:
nothing is collected, which is rare enough that it is worth saying loudly.

**The Office server cannot go in at all.** It is a background service. So the
App Store build is the free app, and the server stays a direct download
whatever happens.

## The licensing problem, which is the one to think hardest about

The free app is free, so there is no in-app purchase and nothing to share
revenue on. Good.

But an Office licence unlocks what the app does when it connects to a licensed
server. Apple's rules on unlocking functionality bought elsewhere have moved
around for years and are still moving. It is defensible — the purchase is for
the *server*, which is not an Apple product — but it is a conversation with a
reviewer, not a certainty.

**Which is the real argument for the shape you have already chosen:** ship
Developer ID from your own site, where none of this applies, and treat the App
Store as a shop window for the free app if you ever want one.

## What you would actually gain

Search. Someone typing "PDF takeoff" into the App Store. That is it — not
trust (notarisation already gives you that), not distribution (your site works),
not payment (Square works).

Worth it eventually. Not worth it before the Windows and Mac builds are
steady, and not worth forking the release process for.

## If you decide to do it

1. Apple Distribution + Mac Installer Distribution certificates.
2. A build feature that compiles out self-update.
3. Move the profiles folder into the container.
4. `entitlements-appstore.plist` instead of `entitlements.plist`.
5. `productbuild` a `.pkg` rather than a `.dmg`.
6. Upload with `xcrun altool` or Transporter.
7. Expect one rejection. Everybody gets one.
