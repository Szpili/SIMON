# SIMON — offline receipt verifier

Every inference job produces an Ed25519-signed receipt bound to its output. The
live demo lets you tamper with a node's answer and watch the receipt get
**REJECTED** — then restore it and it turns **RECEIPT VALID**.

**Live:** https://szpili.github.io/SIMON/ (enable GitHub Pages with source
"GitHub Actions")
**Repo:** https://github.com/Szpili/SIMON

## Verification is Rust, not JavaScript

There is no backend and **no second crypto implementation**. `src/lib/verify.ts`
calls `simon-verify-wasm`, which is `simon-core` (the same code the node harness
and the CLI use) compiled to WebAssembly. It checks:

1. the Ed25519 signature over the digest of the **whole receipt**
   (`job_id`, `model_hash`, `runtime`, `precision`, `output_digest`, …),
2. `output_digest` recomputed from `job_id + output` with the domain separator
   `SIMON/OUTPUT/v1` — so a receipt from another job, or an edited answer,
   is rejected.

The page only displays the verdict. It makes no network calls. (It loads Inter /
JetBrains Mono from Google Fonts; with no network it falls back to system fonts.)

The fixture is **not invented**: `src/lib/fixture.ts` is generated from the real
`przyklady/receipt.json` + `przyklady/odpowiedz.txt` — the same files the CLI
verifies with `simon verify`.

## Develop

```sh
npm install
npm run wasm        # build ui/src/lib/wasm/ from Rust (needs cargo + wasm-bindgen)
npm run fixtures    # regenerate src/lib/fixture.ts from przyklady/
npm run dev         # dev server
npm run build       # static site in build/ (base path /SIMON)
npm run preview     # serve build/ at http://localhost:4173/SIMON/
npm run build:local # build with no base path, to open build/index.html from disk
```

### Building the WASM verifier

`npm run wasm` runs `scripts/build-wasm.sh`:

```sh
rustup target add wasm32-unknown-unknown
cargo install -f wasm-bindgen-cli --version 0.2.108   # must match Cargo.lock
npm run wasm
```

The generated `src/lib/wasm/` files are committed so the static site build needs
no Rust toolchain. Regenerate them whenever `crates/simon-core` or
`crates/simon-verify-wasm` changes.

## Deploy

Push to `main`. The workflow `.github/workflows/deploy-ui.yml` builds and
publishes `build/`. In the repo settings, set Pages source to **GitHub Actions**.
