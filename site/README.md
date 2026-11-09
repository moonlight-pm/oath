# site — oath.moonlight.pm

The public page: what Oath is and its goals (`/`), and the manual
(`/manual`), which is `docs/manual/*.md` rendered as one page. A small
Rust program in its own Cargo workspace, so its crates stay out of the
root `Cargo.lock` and `pkg:oath`, writes it all as plain files into
`dist/`, and Cloudflare serves them as a Worker with static assets (R79).
Not part of the OS.

| Path | What |
|------|------|
| `pages/` | `page.html` (the shell), `home.html`, `manual.html`, `missing.html`; `{{name}}` is filled in by `src/site.rs` |
| `src/manual.rs` | Markdown to HTML (pulldown-cmark): chapters numbered, headings anchored, links between chapters made anchors, other relative links sent to GitHub, raw HTML shown as text |
| `src/main.rs` | Writes every file into `dist/` |
| `samples/` | `session.txt` and `index.md`, recorded on canto, shown on the home page |
| `static/` | `style.css`, `_headers` (security and caching headers), the favicons (SVG, and `favicon.ico` and `apple-touch-icon.png` rendered from it for browsers that take only raster icons), fonts (OFL, licences beside them) |
| `cloudflare.config.ts`, `wrangler.config.ts` | The Worker and where its files are |
| `package.json`, `package-lock.json` | Pin `cf` and the Wrangler it builds with |

The seal is `brand/oath.svg`, drawn in `currentColor`.

Everything is compiled into the generator, so `dist/` is the manual of
the commit it was built from.

## Keeping it true

- A new manual chapter goes in `CHAPTERS` in `src/manual.rs`, in the
  order of `docs/manual/README.md`'s table; `cargo test` fails until
  they match, or if an anchor on the page has no target.
- The home page's Status section and the counts in its catalog table
  are dated. Update them with `docs/capabilities.md`, and re-record
  `samples/` from canto when `oath`'s output changes.

## Local

```sh
cd site
cargo test
cargo run          # writes dist/
```

## Deploy

`.github/workflows/site.yml` tests and builds on every pull request that
touches the site, the manual or the seal, and deploys on a push to
`master`. So the public manual follows `master`, which can be ahead of
what a machine runs. On an Oath machine `npx cf` fails (there is no
`/usr/bin/env`), so `cf` runs only in the workflow.

It reads `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` from the
repo's secrets. The public name is the repo's `SITE_DOMAIN` variable, and
never goes in the repo; without it the site deploys to its `workers.dev`
address only.
