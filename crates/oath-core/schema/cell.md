# cell

One project's own software, made to run in a cell: a program in a box of
its own, which sees the packages it runs with and nothing else of the
machine (`cell run cell:<name>`). Bits live under
`/oath/store/cell/<name>/<hash>/`.

A cell is a `pkg` that stays home. It is not linked into `/bin` or `/lib`,
it comes from no origin, and `oath publish` refuses it, so a project's own
code cannot reach a public store by mistake. Its name cannot clash with a
package's.

**When to use:** a server or other long-running program that belongs to
one project.

**When not:** software any machine could use is a `pkg`. A cell does not
start itself: a `svc`, or whatever runs the project, runs `cell run`.

## Fields

- `present` — `true` makes `hash` the tree `cell run` starts. `false` means
  `cell run` refuses it. The store tree stays.
- `hash` — the pin, required while present: `sha256-` plus 64 lowercase hex
  of the tree. Apply hashes the tree against it. The tree must be in the
  store already, by `oath store cell:<name>` or `oath build`.
- `needs` — the packages it runs with, each `pkg:<name>`, named and not
  pinned: a cell starts with each at the hash this machine links, so moving
  a package leaves no cell behind. Apply refuses a need this machine lacks,
  and refuses to remove a package a present cell needs.
- `description`, `home`, `source` — as for a `pkg`. Not part of any hash.

A plan makes one with `"produces": "cell:<name>"`, and may build from
`cell:<name>-src`, so a project's source stays off every origin too. A
`pkg`'s plan cannot name a `cell:`.

## Example

    sudo oath store cell:hamlet ./out      # cell:hamlet sha256-…
    sudo oath new cell:hamlet hash=sha256-… 'needs=["pkg:node"]'
    sudo oath apply cell:hamlet
    sudo -u work cell run --name hamlet -p 8080:8080 -v /work/hamlet:/data cell:hamlet

The program is `bin/<name>` in the tree, unless `cell run` names another.
What should last goes to a `-v` volume: the rest of its tree is gone when
it stops.
