# site — oath.moonlight.pm

The public page: what Oath is and its goals (`/`), and the manual
(`/manual`), which is `docs/manual/*.md` rendered as one page. A small
Rust server in its own Cargo workspace, so its crates stay out of the
root `Cargo.lock` and `pkg:oath`. Not part of the OS.

| Path | What |
|------|------|
| `pages/` | `page.html` (the shell), `home.html`, `manual.html`, `missing.html`; `{{name}}` is filled in by `src/site.rs` |
| `src/manual.rs` | Markdown to HTML (pulldown-cmark): chapters numbered, headings anchored, links between chapters made anchors, other relative links sent to GitHub, raw HTML shown as text |
| `src/main.rs` | HTTP/1.1, GET and HEAD; ETags and 304s; security headers; `oath.wicket.cloud` redirects here |
| `samples/` | `session.txt` and `index.md`, recorded on canto, shown on the home page |
| `static/` | `style.css`, the favicon, fonts (OFL, licences beside them) |

The seal is `brand/oath.svg`, drawn in `currentColor`.

Everything is compiled into the binary and rendered once at start, so
an image serves the manual of the commit it was built from. A change to
`docs/manual` reaches the site at the next deploy.

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
cargo run          # http://127.0.0.1:8090/ (OATH_SITE_PORT to change)
```

The listen port is the first of `PORT` (Wicket), `WICKET_PORT_HTTP`,
`OATH_SITE_PORT` that is set, else 8090.

## Deploy (Wicket on aulos)

From the repo root:

```sh
podman build -f site/Containerfile --ignorefile site/.containerignore \
  -t oath-site:latest -t registry.wicket.cloud/oath-site:latest .
wicketctl registry push aulos --image oath-site:latest
wicketctl workload apply aulos --file site/workload.toml --confirm
```

The image is the one static binary on `scratch` (about 2 MB), running
as UID 65534; it exits on SIGTERM. The workload keeps the id of the
`oath` workload that Thoxa's `workloads/oath` created.
