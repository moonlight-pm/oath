# plan

A **plan** is how a pack is built. Name is a slot; hash is the file.
Store `/oath/store/plan/<name>/<hash>/plan.plan`. One plan produces
`pkg:<name>`, or `cell:<name>`. `oath build` writes the product tree into the store; it
does not link `/bin`.

## Fields

- `hash` — pin of the canonical plan file (`sha256-` + 64 hex).

## Verbs

- `oath fmt <path>` — write the only legal form to stdout (`-w` in place).
- `oath build plan:<name>` — pin required; isolated, netless build.
  It creates the product's object (`pkg:<name>`, or `cell:<name>` for a
  plan that produces a cell) if there is none; `--no-pkg` does not, for a
  product that is only a build input to other plans.
- `oath build --file <path>` — ad-hoc; does not copy into `store/plan/`.
- `oath set plan:<name> --from-file <path>` — lint, store, pin.

The script's output goes to `/oath/log/plan/<name>.log` (the build
before is `.1`); `oath logs plan:<name>` reads it. A failed build
prints the log's last lines.

Every `build_needs` pack must be in the store at its pinned hash.
A missing package is fetched **before** the sandbox starts: from its
`pkg` object's `url`, else from `host:local.origins`. Network is off
inside. The object the build makes has `present` false; installing it
is `oath set <id> present=true` then apply.
A plan that produces a cell may also build from `cell:<name>` trees,
which come from no origin; a package's plan may not, since its source
is published with it.
A recipe (R23: a package the image may not carry, `pkg:claude-code`)
is such a plan in every catalog; its source is fetched this way, from
the vendor.

The plan's actual records its last build: `product` (the tree's hash)
and `inputs` (a digest of its `build_needs`). That record is not a
proof; the store's hashes are.
