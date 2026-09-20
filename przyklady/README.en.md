# Check it yourself, without running the network

A real receipt from a SIMON node and the response it refers to. Verification is
**offline** — no network, and no trust in whoever handed you these files.

```bash
cargo build --release
./target/release/simon --verify-receipt przyklady/receipt.json \
                       --expect-output przyklady/odpowiedz.txt
```

It should print `RECEIPT WAŻNY` (RECEIPT VALID) and exit with code 0.

Now try to cheat:

```bash
cp przyklady/odpowiedz.txt /tmp/podmieniona.txt
printf ' ' >> /tmp/podmieniona.txt        # ONE extra space

./target/release/simon --verify-receipt przyklady/receipt.json \
                       --expect-output /tmp/podmieniona.txt
# treść wyniku : NIEZGODNA — to nie jest ten wynik
#                (output content: MISMATCH — this is not that output)
# werdykt      : ODRZUCONY        (kod wyjścia 1)
#                (verdict: REJECTED — exit code 1)
```

## What this proves, and what it does not

**It proves:** a registered node signed **exactly this response** as the output
of **this request**. Substituting the content, swapping it in transit, and
planting a receipt from a different execution are all rejected.

**It does NOT prove:** that the declared model generated it. A malicious node can
sign arbitrary text together with a correctly computed `output_digest`. Binding
`model + prompt + execution + output` requires an execution audit, which **does
not exist** — see `docs/SECURITY-LOG.en.md`.

The token counters in the receipt are a **declaration by the node**: the
signature makes it impossible to change them later, but it does not make them
true.

## A note on backward compatibility

Receipts issued **before 18 September 2026** will not pass this verification.
That is when the output digest gained a domain separator (`SIMON/OUTPUT/v1`), so
the commitment format changed. A receipt does not yet carry a schema version, so
an old receipt is rejected with the message "treść niezgodna" (content mismatch)
rather than "nieznana wersja" (unknown version) — this is a **known debt**,
spelled out as M5.1c in `ROADMAP.md`.
