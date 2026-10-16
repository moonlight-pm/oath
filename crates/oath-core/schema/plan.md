# plan

A **plan** is how a pack is built. Name is a slot; hash is the file.
Store `/oath/store/plan/<name>/<hash>/plan.plan`. One plan produces
`pkg:<name>`. `oath build` writes the product tree into the store; it
does not link `/bin`.

## Fields

- `hash` — pin of the canonical plan file (`sha256-` + 64 hex).

## Verbs

- `oath fmt <path>` — write the only legal form to stdout (`-w` in place).
- `oath build plan:<name>` — pin required; isolated, netless build.
- `oath build --file <path>` — ad-hoc; does not copy into `store/plan/`.
- `oath set plan:<name> --from-file <path>` — lint, store, pin.

The script's output goes to `/oath/log/plan/<name>.log` (the build
before is `.1`); `oath logs plan:<name>` reads it. A failed build
prints the log's last lines.

Every `build_needs` pack must be in the store at its pinned hash.
A missing one whose `pkg` object has a `url` is fetched **before** the
sandbox starts. Network is off inside. The product's `present` stays
false; installing it is `oath set pkg:<name> present=true` then apply.

`built_from` on the **product** is not a proof. Hashes are.
