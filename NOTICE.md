# Excalibur View is source-available under the Elastic License 2.0

The licence itself is in `LICENSE.txt`. This is the plain-words version, which
is not the licence and does not replace it.

## What you may do

Read this source, change it, and build and run it yourself — alone or across
your company.

## What you may not do

- Offer it to others as a hosted or managed service.
- Work around, disable or remove the licensing.
- Strip the copyright and licence notices.

## This is not open source

The Elastic License 2.0 is not an OSI-approved open-source licence, and
calling it one would be misleading. It is **source-available**. We would
rather say what it is.

## Why publish it at all

So a fabricator's compliance officer can read every line rather than take our
word for what the program does with their drawings, and so an IT department
can build it themselves if their rules say they must — without handing a
competitor the product.

## What is not in here

- **The Office server.** Commercial, and closed.
- **The trade tool chests we sell** — structural steel and misc metals,
  concrete and earthwork, electrical, mechanical, plumbing, general
  contractor — and the tables behind them. The chest *format* is open, and so
  is reading and writing one: build your own, for yourself or for your
  company, and the program will load it like any other. What is sold is the
  research, not the file format.
- **The signing keys.** Releases are signed with a key held offline. The
  public half is in `crates/hyperview/src/trust.rs` — that is how the program
  checks an update before installing it — and the private half has never been
  on a computer that faces a network.

## The PDF engine

The program uses PDFium, which is BSD-licensed and is not included here.
`crates/hyperview/build.rs` picks it up from `third_party/pdfium/embedded/`
if it is there, and the program looks for it beside itself if it is not.

Excalibur Construction Technologies · Pueblo, Colorado · hello@excaliburct.com
