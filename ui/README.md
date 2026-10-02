# SIMON

Every inference job produces an Ed25519-signed receipt. Anyone can re-run one forward pass and audit it offline.

This repo is the live demo: open the page, tamper with a node's output, and watch the receipt get **REJECTED**. Restore it and it turns **RECEIPT VALID**.

![Screenshot placeholder](docs/screenshot.png)

**Live:** https://szpili.github.io/SIMON/ (available once GitHub Pages is enabled with source "GitHub Actions")
**Repo:** https://github.com/Szpili/SIMON

## Verification runs client-side

There is no backend. `verifyReceipt()` in `src/lib/crypto.ts` runs in your browser using `@noble/hashes` (SHA-256) and `@noble/ed25519`:

1. Recompute `sha256(output)` and compare to `output_sha256`.
2. Verify the Ed25519 signature over `output_sha256` against `node_pubkey`.
3. Valid only if both pass.

Verification makes no network requests. (The page itself loads Inter and JetBrains Mono from Google Fonts; with no network it falls back to system fonts.)

## Develop

```sh
npm install
npm run dev            # dev server
npm run build          # static site in build/ (base path /SIMON)
npm run preview        # serve build/ at http://localhost:4173/SIMON/
npm run build:local    # build with no base path, to open build/index.html from disk
npm run fixtures       # regenerate real keys + signatures into src/lib/fixture.ts
```

## Deploy

Push to `main`. In the repo settings, set Pages source to **GitHub Actions**. The workflow in `.github/workflows/deploy.yml` builds and publishes `build/`.
