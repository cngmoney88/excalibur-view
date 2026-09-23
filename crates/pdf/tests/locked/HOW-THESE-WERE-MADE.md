# The locked files in this folder

Small synthetic drawings, one page, made here and locked by **qpdf** — a
different program from this one, which is the whole point. A decryptor tested
only against files its own encryptor wrote proves nothing except that it is
self-consistent.

One source file, `plain.pdf`, holds a title (`Fox Theater Structural`) and a
content stream reading `MESA FAB SHEET S-101`. Everything else is that file
locked five ways:

| File | Handler | Made with |
|---|---|---|
| `rc4-40.pdf.b64` | V1 R2, RC4 40-bit | `qpdf --allow-weak-crypto --encrypt --user-password=bolt --owner-password=creede --bits=40 --` |
| `rc4-128.pdf` | V2 R3, RC4 128-bit | as above with `--bits=128 --use-aes=n` |
| `aes-128.pdf` | V4 R4, AESV2 | `--bits=128 --use-aes=y` |
| `aes-256.pdf` | V5 R6, AESV3 | `--bits=256` |
| `owner-only.pdf` | V5 R6, no user password, printing denied | `--user-password= --owner-password=creede --bits=256 --print=none` |

The user password is `bolt` and the owner password is `creede`, except in
`owner-only.pdf`, which opens with an empty password and is the shape most
"you may not print this" files in circulation actually have.

Worth noting: qpdf refuses to write the first two at all without
`--allow-weak-crypto`. It is right to. We read them anyway, because a general
contractor sends what a general contractor sends, and we say on screen that
the lock is decorative.

Nothing here contains anybody's drawings. They are a few hundred bytes of
made-up text.
