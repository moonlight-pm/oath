# plan

A **plan** is how a pack is built. Name is a slot; hash is the file.
Store `/oath/store/plan/<name>/<hash>/plan.plan`. One plan produces
`pkg:<name>`. `oath build` writes the product tree; it does not link
`/bin`.

## Fields

- `hash` — pin of the canonical plan file (`sha256-` + 64 hex).

## Verbs

- `oath fmt <path>` — write the only legal form to stdout (`-w` in place).
- `oath build plan:<name>` — pin required; isolated netless build.
- `oath build --file <path>` — ad-hoc; does not copy into `store/plan/`.
- `oath set plan:<name> --from-file <path>` — lint, store, pin.

`built_from` on the **product** is not a proof. Hashes are.
