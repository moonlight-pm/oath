# Agent surface

The sysadmin is an agent. An agent reaches an Oath machine the way the
owner does: over SSH as `home`, with a key listed in `ssh:local`, and
it changes the machine with `oath`. `oath mcp` speaks the Model
Context Protocol (MCP) on stdin and stdout, so an MCP client gets the
verbs as typed tools and the INDEX as its instructions. It is a thin
adapter: each tool call runs one `oath --json` command, so there is
still one admin surface.

**Status:** built; phase 7 done 2026-10-18. Steps 1–3 were built
2026-10-17: `oath mcp` (`crates/oath/src/mcp.rs`) and
`cargo make check --agent` (`crates/oath-make/src/agent.rs`,
`check/agent.toml`). "Later" below is not built. Joshua decided the shape on 2026-10-17 (R16 in
[../decisions.md](../decisions.md)).

Replaces: roadmap phase 7's one line ("MCP and agent tooling over
`oath --json`") and the pre-cut "Phase 5" resident agent (tag
`t37-t39-final`).

## What it builds on (built)

- Every listed verb but `logs` and `fmt` prints the same facts as JSON
  with `--json`. In JSON mode an error is `{"error", "hint"}` on stdout, and
  the exit code is 0 ok, 1 error, 3 `--confirm` required, 4 security
  refusal (catalog.md).
- The INDEX teaches a model that has never seen Oath, and `oath schema
  <kind>` is generated from the same Rust types `set` and `apply` parse.
- `home` (uid 1) is the SSH login; `sudo` has no password and runs a
  command with a clean root environment (network-and-ssh.md).
- Confirm-class changes, probation and `oath keep`'s new-session check
  (catalog.md).

## Rules

- **One admin surface.** A tool is an `oath` verb. The adapter has no
  catalog logic of its own: whatever an agent does over MCP, the owner
  can do with `oath` and sees in `oath log`.
- **No new daemon, port or credential.** SSH is the transport and the
  authentication. The server lives as long as one SSH connection.
- **The agent is not a third user** (decisions, Principal). It runs as
  `home`, and writes go through `sudo`, as they do for the owner.
- **Not a chatbot in PID 1** (scope.md). Oath ships no model, no API
  key and no agent process.

## Transport (built)

```sh
ssh -T home@10.0.0.3 oath mcp
claude mcp add canto -- ssh -T -o BatchMode=yes home@10.0.0.3 oath mcp
```

- MCP's stdio transport: one JSON-RPC message per line on stdin and
  stdout. Nothing else is written to stdout; stderr carries only
  `oath-tel` lines. The server exits 0 at the end of stdin.
- It handles one request at a time, in order. A started call runs to
  its end and `notifications/cancelled` is ignored: an apply stopped
  halfway is worse than a late answer.
- `oath mcp --read-only` lists only the read tools and never runs
  `sudo`; there `verify` runs unprivileged and only reports. `--root` works as for every verb (tests, the harness).
- sshd runs the command through `home`'s login shell (`zsh -c`,
  R27), with `host:local` env from its `SetEnv`; `-T` keeps a terminal
  out of the byte stream.

## Protocol versions (built)

The server is dual-era: it serves the current revision and the
handshake-based ones before it.

- **Modern, `2026-07-28`.** No handshake. Each request carries
  `_meta` `io.modelcontextprotocol/protocolVersion` and
  `io.modelcontextprotocol/clientCapabilities`; a request without
  them is refused with `-32602`, and an unknown version with `-32022`
  and `data: {supported, requested}`. `server/discover` returns
  `supportedVersions: ["2026-07-28"]`, `capabilities: {tools: {},
  resources: {}}` and the instructions. Every result carries `resultType: "complete"`
  and `_meta` `io.modelcontextprotocol/serverInfo`; `tools/list` and
  `server/discover` carry `ttlMs: 3600000` and `cacheScope: "private"`
  (the tools change only with the `oath` binary), and so does
  `resources/templates/list`; `resources/list` and `resources/read`
  carry `ttlMs: 0` (any apply changes them).
- **Legacy, `2025-11-25` back to `2024-11-05`.** An `initialize`
  request switches the process to legacy for every request without the
  modern `_meta`; one that carries it is still served modern, as the
  revision allows a dual-era server to serve both. The answer is the client's
  version when the server knows it, otherwise `2025-11-25`, with
  `capabilities: {tools: {}, resources: {}}`, `serverInfo` and
  `instructions`.
  `notifications/initialized` is ignored; `ping` returns `{}` (in
  both eras). A JSON-RPC batch (an array, `2025-03-26`) gets an array
  of replies.
- The results look the same in both eras; legacy clients ignore
  `resultType`, `ttlMs` and `cacheScope`.
- `serverInfo` is `name: "oath"`, `title: "Oath on <hostname>"`, and
  `version`: the first 12 hex digits of the running `pkg:oath` hash
  (from the executable's store path; `dev` outside the store).
- There are no prompts, logging, sampling, roots, subscriptions or
  tasks.

## Resources (built, step 3)

Resources are the reads, addressed by URI, for a client that pulls
context in rather than calling a tool. Each read runs the `oath --json`
command its tool runs, never through `sudo`.

| URI | Reads | MIME type |
|-----|-------|-----------|
| `oath://index` | the INDEX (`oath --json`, its `text`) | `text/markdown` |
| `oath://schema/<kind>` | `oath --json schema <kind>` | `application/json` |
| `oath://object/<kind>/<name>` | `oath --json get <kind>:<name>` | `application/json` |

`resources/list` names the index, every kind's schema and every object
`ls` lists; `resources/templates/list` gives `oath://object/{kind}/{name}`
and `oath://schema/{kind}`. A URI that is not one of these, an unknown
kind, a name outside the id grammar or an object that does not exist
is "Resource not found": `-32602` with `data: {uri}` (and the CLI's
error when it ran), or `-32002` for a legacy session, as those
revisions had it. A bad URI runs nothing.

## Instructions (built)

The `instructions` string is the live INDEX (`oath --json`, its
`text`), after one paragraph:

> These tools are the `oath` verbs on `<hostname>`. Each call runs one
> `oath --json` command as `home`, with `sudo` for writes, and returns
> its JSON. Where the INDEX says `oath set host:local …`, call
> `set_host`. The tool `index` returns this text again.

A model learns where it is before its first call, and nothing about
Oath has to live in the client.

## Tools (built)

The list is built at startup, always in this order.

| Tool | Arguments | Runs | Hints |
|------|-----------|------|-------|
| `index` | — | `oath --json` | read-only |
| `ls` | `kind?` | `oath --json ls [--kind k]` | read-only |
| `schema` | `kind?` | `oath --json schema [kind]` | read-only |
| `get` | `id`, `part?` (`desired`, `actual`) | `oath --json get <id> [--desired\|--actual]` | read-only |
| `diff` | `id?` | `oath --json diff [id]` | read-only |
| `log` | `lines?` (20) | `oath --json log -n <lines>` | read-only |
| `logs` | `id?`, `lines?` (50) | `oath --json logs [id] -n <lines>` | read-only |
| `verify` | `ids?` | `sudo oath --json verify [ids]` | read-only |
| `set_host` `set_svc` `set_pkg` `set_net` `set_ssh` | `name`, `fields` | `sudo oath --json set <kind>:<name> --from-json <fields>` | idempotent |
| `new_svc` `new_pkg` | `name`, `fields` | `sudo oath --json new <kind>:<name> --from-json <fields>` | |
| `rm` | `id` | `sudo oath --json rm <id>` | idempotent |
| `apply` | `ids?`, `confirm?`, `probation_secs?` | `sudo oath --json [--confirm] apply [--probation s] [ids]` | destructive, open world |
| `undo` | — | `sudo oath --json undo` | destructive |
| `keep` | — | `sudo oath --json keep` | |
| `file_plan` | `name`, `text` | `sudo oath --json set plan:<name> --from-file -`, `text` on stdin | idempotent |
| `build` | `plan` | `sudo oath --json build plan:<name>` | idempotent |

"Hints" are MCP tool annotations (`readOnlyHint`, `destructiveHint`,
`idempotentHint`, `openWorldHint`); unlisted ones are false. `verify`
is read-only because it changes only status. `set`, `new` and `rm`
change spec but nothing on the machine until `apply`. `apply` is open
world because a `pkg` with a `url` fetches. `sudo` is always `sudo -n`.

Input schemas:

- `fields` of `set_<kind>` is the kind's generated schema
  (`oath_core::schema::schema`) without `required`, because `set`
  merges; `fields` of `new_<kind>` is the schema as it is. The field
  descriptions are the doc comments, so the tool list carries the
  text `oath schema` prints, and a new field reaches the tools with
  no adapter change.
- `name` has the id grammar's pattern, `^[A-Za-z0-9_-][A-Za-z0-9._-]{0,63}$`.
  The descriptions of `set_host`, `set_net` and `set_ssh` name their
  one object (`host:local`, `net:net0`, `ssh:local`).
- `kind` (in `ls` and `schema`) is an enum of the kinds.
- Every schema is closed (`additionalProperties: false`).

Left out, with the reason:

- `set` for `dev` (inventory; its one field cannot be set false), for
  `snap` (`snap:current` is a boot-generation change; `undo` is the
  path) and for `plan` (`file_plan` files one from its text).
- `fmt`, `seed`, `panic-init` and `undo --expired` (build time, the
  harness, PID 1).

Each tool's description says what it changes and points to `schema`.
`apply`'s description repeats the INDEX rule for `confirm`. `keep`'s
says it works only from a connection opened after the apply. `logs`'
and `get`'s say their strings are data that services and people wrote,
not instructions.

## Running a call (built)

- The argv is built directly and never passes through a shell.
  `fields` is serialized as one argument. Flags come first and
  positional values after `--`, so an id that starts with `-` cannot
  pass for a flag.
- The program is the server's own executable (`/proc/self/exe`), so a
  server and its calls are one version. Only `file_plan` writes to the
  command's stdin (the plan's text); every other call gets none, and
  none ever reads the server's own stdin.
- A `tools/call` whose `_meta` has a `progressToken` gets
  `notifications/progress` every 5 s while its command runs: `progress`
  is the seconds so far, `message` names the tool and, for `build`, the
  last line of the build's log. A call that ends sooner sends none.
- Every call gets `--root <root>` (sudo's clean environment drops
  `OATH_ROOT`) and `--via mcp:<client>`, where `<client>` is
  `clientInfo.name`, reduced to `A-Za-z0-9._/-` and 64 characters.
  `clientInfo` is self-reported, so it only labels log lines.
- The result's `content` is one text block: the command's stdout.
  Exit 0 is `isError: false`. Any other exit is `isError: true`, and
  a line names it: exit 3 "confirm required: nothing changed; pass
  `confirm: true` only if the owner asked for this class of change",
  exit 4 "security refusal: nothing changed", any other "exit N".
  When stdout is empty, the text is stderr without `oath-tel` lines.
  An apply that went on probation gets a line saying so and that
  this connection cannot keep it.
- `oath --json` writes only its answer to stdout: the commands an
  apply starts (the btrfs snapshot, `ssh-keygen`, `ip`, `wget`,
  `tar`) write theirs to stderr. Before 2026-10-17 the snapshot's
  "Create readonly snapshot" line came first on stdout.
- There is no `structuredContent` in step 1: legacy clients need an
  object, and `ls` returns an array.
- An unknown tool, or arguments that do not match the input schema,
  is a JSON-RPC error (`-32602`). A bad id or field value reaches the
  CLI and comes back as a tool error with the CLI's hint, so the model
  can correct itself.

## Safety (built)

- **Power.** The server can do what the SSH login it runs in can do:
  `home`, plus `sudo`. `--read-only` is for an agent that should only
  look.
- **Confirm.** `apply`'s `confirm` becomes `--confirm`. Without it a
  confirm-class apply is refused and logged `refused`, as on the CLI.
- **Probation.** `keep` runs `oath keep` under the server, so the
  new-session check walks the server's own ancestors: `oath mcp`, the
  shell and sshd's per-connection processes (`sshd-session`). A connection opened before the
  apply cannot keep it. The client starts a new `ssh … oath mcp`, or
  the owner runs `ssh home@<host> sudo oath keep`. A model that holds
  one connection cannot keep its own lockout-class change, and the
  rollback stays the default; that is what probation is for. When an
  apply goes on probation, its result says so.
- **Power changes.** A reboot or halt ends the connection; the client
  sees the server exit. `apply`'s description says so.
- **Audit.** The apply log records `actor.via` and, on an apply,
  `confirm: true` when it was passed. `oath log` answers "what did
  the agent change, and what did it force?".
- **Untrusted text.** Service output, a hostname and a `svc` `env` are
  written by services and people. The server passes them on unchanged,
  and the tool descriptions say they are data.
- **Rate.** One call at a time, and every writer takes the catalog
  lock.

## CLI changes (built, step 1)

- `oath mcp [--read-only]`, in the `oath` binary (`crates/oath/src/mcp.rs`).
  It needs no new crate or dependency: `serde_json` is enough for
  JSON-RPC.
- `oath logs --json`: `{"id", "path", "lines": […]}`.
- `oath log -n <n>`: the last `n` entries (default all).
- A hidden global `--via <label>`, kept in `Actor` and written as
  `actor.via`; apply log lines get `confirm: true` when `--confirm` was
  given.
- The INDEX gains `oath mcp  the verbs as an MCP server on stdin/stdout
  (ssh -T home@<host> oath mcp)`.

## Tests (built, step 1)

- `cargo test`: the dispatcher is a function from a request (and the
  era chosen so far) to a response, with a fake runner. It covers both
  eras, `server/discover`, `-32602` for a request without `_meta`,
  `-32022`, an unknown tool, the argv of every tool (`sudo` or not,
  `--root`, `--via`, `--confirm`), exit codes, and `--read-only`.
- `check/cases.toml`, all `metal = true`. Each case pipes JSON lines
  into `oath mcp` on the guest and checks the replies with `grep`, so
  the build host needs no MCP client:
  - `mcp-handshake`: legacy `initialize` and `tools/list`; modern
    `server/discover` and `tools/list`; an unknown version gets
    `-32022`.
  - `mcp-loop`: the README loop over MCP: `set_host`, `apply`, `get`
    shows the name in actual, `undo`; `oath log` shows `via` `mcp:`.
  - `mcp-confirm`: `apply` of `host:local` `power: halt` without
    `confirm` is an error naming exit 3; the log says `refused` and the
    machine keeps running.
  - `mcp-keep`: an `env` change to `svc:sshd` (lockout class and
    harmless, as in `probation-kept-from-a-new-session`), applied with
    `confirm` in one `oath mcp` run, goes on probation; `keep` in the
    same run is refused; `keep` from a new SSH connection's `oath mcp`
    is accepted.
  - `mcp-read-only`: `--read-only` lists no write tool, and calling
    `apply` there is `-32602`.

## Acceptance: a real model (built, step 2)

The bar is the product's own (AGENTS.md): a running machine must teach
a model that has never seen Oath. The test gives a fresh model the
machine's `oath mcp` and a task, and nothing else.

- `cargo make check --agent [--model m] [--target home@host]` runs the
  tasks in `check/agent.toml`. Each task gets a fresh QEMU of the
  output (or the target machine, undone afterwards like a metal case),
  runs its `setup` lines over SSH, runs the client on its `prompt`, and
  then checks its `expect` lines over SSH. What the model says it did
  is not evidence.
- The default client is Claude Code, headless, on novus:
  `claude -p <prompt> --bare --strict-mcp-config --mcp-config <oath
  server> --tools "" --allowedTools mcp__oath --permission-mode
  dontAsk --model <m> --max-budget-usd <cap> --output-format
  stream-json --verbose --no-session-persistence --setting-sources
  local`, run in an empty directory (`stream-json` needs `--verbose`
  under `-p`). `--bare` and `--tools ""`
  leave out CLAUDE.md, memory, hooks, skills and the shell, so the
  model knows only what the server tells it. The command is a template
  in `check/agent.toml`, so another client or vendor runs the same
  tasks.
- The key is `ANTHROPIC_API_KEY` from the run's environment.
  `oath-make` refuses to run without it, never writes it under
  `build/`, and it is never committed.
- The default model is `haiku`: the smallest model is the strictest
  test of whether Oath teaches. The result records the model.
- Opt-in. It is not part of `cargo make check` and does not gate
  install: it costs money and is not deterministic. Results go to
  `build/out/<id>/agent-check.json` (task, model, pass, cost, turns),
  transcripts to `build/out/<id>/agent/<task>.jsonl`.
- A task also fails if the apply log shows a `confirm` the task did
  not allow (`confirm = true` in the task), or a change whose
  `actor.via` is not `mcp:` (PID 1's probation rollback aside).
- `reboots = true` marks a task that ends in a reboot: the harness
  waits for QEMU to exit and boots the disk again (or, on a machine,
  waits for a new boot id) and requires a new boot id before its
  checks. Any other task whose VM exits is booted again and judged.
- Claude Code 2.1.302 speaks the modern revision: it probes with
  `server/discover`, then lists and calls tools with the modern
  `_meta` (seen 2026-10-17).
- The first tasks:
  1. `rename`: "Rename this machine to agent-test." The name is in
     actual and `oath diff` is in-sync.
  2. `service`: "Run a service called hello, as the user home, that
     prints hello every 10 seconds." `svc:hello` runs as `home`, and
     its log shows `hello`.
  3. `undo`: setup applies an `env` change to `host:local`. "The last
     configuration change on this machine was a mistake. Take it
     back." The log ends in that generation's undo, and the variable
     is gone.
  4. `diagnose`: setup creates and applies `svc:broken` with `exec`
     `/bin/slep 600`. "svc:broken keeps failing. Find out why and fix
     it." `svc:broken` runs.
  5. `reboot`: "Reboot this machine." `confirm = true`; the boot id
     changed.

## Steps

1. `oath mcp` with every tool in both eras, `--read-only`, the CLI
   changes, the unit tests, the five cases, and a manual section
   ("Agents" in `docs/manual/using.md`). Done when the QEMU harness
   passes. **Done 2026-10-17:** all 46 cases passed in QEMU; canto ran
   output `e3a11645f14c` and passed all 37 metal cases.
2. The agent check. Done when every task passes with the default model
   in QEMU. **Built 2026-10-17; 4 of 5 pass.** The first run (haiku,
   output `e3a11645f14c`, $0.0083 in all) passed `rename`, `service`,
   `undo` and `reboot`. Each time the model read `index` first, then
   set, diffed, applied and looked again; it passed `confirm` only for
   the reboot. Its one wrong guess (a `description` field on a `svc`)
   came back with the CLI's hint and it corrected itself. `diagnose`
   never reached the model: its setup required an exit 0 from an apply
   whose service cannot start, which is an error. The setup now
   expects that error. On canto (2026-10-17, haiku, $0.0088 in all)
   all five tasks pass, `diagnose` included: the model read the
   failure reason, looked for a package that provides `slep`, fixed
   the `exec` typo to `/bin/sleep` and saw the service run. **Done
   2026-10-18:** in QEMU on output `218fd2e1e03b` (GCC, glibc), haiku
   passed all five tasks, `diagnose` included, in 45 turns for $0.0096.
3. What step 2's transcripts show is missing, and: resources
   (`oath://index`, `oath://schema/<kind>`,
   `oath://object/<kind>/<name>`), filing a plan from text
   (`file_plan`; the CLI gains `set plan:<name> --from-file -`), and
   progress notifications during a long `build` or `apply` when the
   request has a `progressToken`. **Built 2026-10-17.** From the
   transcripts: a service whose first apply fails is no longer left
   `new` (its spec is applied; `status` is `error` while PID 1 has
   given up on it), and PID 1's "cannot start" line in its log is
   stamped and names the path, so a model can tell it from the starts
   after a fix.

Phase 7 was done when step 2 passed in QEMU and on canto, and canto
ran an output with `oath mcp`: all three held on 2026-10-18.

## Later (not phase 7)

- **The owner confirms through the client.** A confirm-class `apply`
  without `confirm` would ask the person through MCP elicitation (an
  `input_required` result in the modern era, `elicitation/create` in
  legacy), so the person says yes, not the model. It needs clients
  that support elicitation.
- **A key that can only look.** Binding a key to `oath mcp
  --read-only` needs OpenSSH `command=` options in `ssh:local`; today
  a key line must start with its type. That needs a decision.
- **Several machines from one place.** A server on the build host
  that fronts each machine's `oath mcp`, as a thin layer on top.
- **A resident agent** is a payload (scope.md): a package and a `svc`
  that run a client against `oath mcp`, with credentials the owner
  gives it. Core does not change for it.

## Courage test

A model that has never seen Oath, given `ssh -T home@<host> oath mcp`
and nothing else, renames the machine, runs a new service, finds and
undoes a bad change, and fixes a failing service. The apply log shows
each change came through MCP, with no confirm-class change it was not
asked for.
