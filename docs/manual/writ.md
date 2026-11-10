# writ

`writ` is Oath's coding agent. A model works on one project from inside
a box with no network, no home and no way to `sudo`, and what it writes
is held in a session until you apply it. The model is a local one served
by `llama-server`, or one on OpenRouter. `writ` runs as `home`.

```
cd ~/hamlet
writ                # type, watch it work, /diff, /apply
```

## Install

`pkg:writ` is on the origin and in no profile. It needs `pkg:bubblewrap`
(which needs `pkg:libcap`) and `pkg:curl`. The `desk` profile has all
three. `base` has `pkg:curl` and `pkg:libcap`, so a `base` machine adds
`pkg:bubblewrap`. Each hash comes from the origin, and each need is
pinned at the hash the plan names, as in [Packages](packages.md):

```
curl -fsS https://store.oath.moonlight.pm/INDEX.md | grep -E '^\| `(writ|bubblewrap|llama-cpp)` '

# desk
sudo oath new pkg:writ --from-json '{"present":true,"hash":"sha256-…","needs":[{"id":"pkg:bubblewrap","hash":"sha256-…"},{"id":"pkg:curl","hash":"sha256-…"}]}'
sudo oath apply pkg:writ

# base: pkg:bubblewrap first, then the same pkg:writ
sudo oath new pkg:bubblewrap --from-json '{"present":true,"hash":"sha256-…","needs":[{"id":"pkg:libcap","hash":"sha256-…"}]}'
sudo oath new pkg:writ --from-json '{"present":true,"hash":"sha256-…","needs":[{"id":"pkg:bubblewrap","hash":"sha256-…"},{"id":"pkg:curl","hash":"sha256-…"}]}'
sudo oath apply pkg:bubblewrap pkg:writ
```

`writ --help` lists the verbs. A model is one more step: see Local
models or OpenRouter below.

## A session

`writ` with no verb, on a terminal, is a session on the current
directory. Off a terminal it prints its usage.

| Flag | Default | |
|------|---------|---|
| `--project DIR` | this directory, or the session's | The project |
| `--session NAME` | a new session, `s` and the time | Name the session, or go on with one |
| `--max-turns N` | 100 | Turns of the model for one prompt |
| `--timeout SECS` | 1800 | For one turn of the model |

A session name is letters, digits, `.`, `_` and `-`, and does not start
with `.`.

| Key | |
|-----|---|
| Enter | Send |
| Alt-Enter, Ctrl-J, or `\` before Enter | A new line |
| Up, Down | Earlier prompts |
| Ctrl-C or Esc | Stop what is running |
| Ctrl-T | Show or hide the model's thinking, hidden by default |
| Ctrl-L | Clear the screen |
| Ctrl-D, or Ctrl-C twice on an empty line | Leave |

| Command | |
|---------|---|
| `/diff [--names]` | What this session would change in your tree |
| `/apply [--all] [PATH]` | Take its changes; what runs later waits for its name, or `--all` |
| `/undo` | Take back what `/apply` did |
| `/use` | Which model |
| `/use local [OPTIONS]` | Switch to the local one, and make sure it is loaded |
| `/use openrouter [MODEL] [--budget DOLLARS]` | Switch to one on OpenRouter |
| `/llama [start\|stop]` | The local server by hand |
| `/sessions` | Every session, and what each holds |
| `/new` | A new session; this one stays as it is |
| `/help` | These |
| `/quit` | Leave |

A command is one line, a slash and a plain lowercase word, so `/etc/passwd
is wrong` is a prompt. A prompt that starts with `//` sends the rest
with one `/`. A paste is only inserted, so it cannot send itself.

The box starts for a prompt and ends with it. Nothing in it runs while
you read what it held, so `/diff` shows what `/apply` takes, and a
process a model started does not outlast its prompt. One job runs at a
time. `/help` and `/quit` act at once; any other command sent while one
runs goes back on the input line, to send again when it stops. A prompt
sent while one runs waits its turn. Ctrl-C or Esc ends the request, or
kills the box if a command is running. What the model had said stays in
the conversation, marked as interrupted.

Everything a model or a command prints passes through one cleaner:
control bytes and invisible characters show as text, such as `^[` and
`⟨U+202E⟩`, so neither can move the cursor or set the title. A model's
prose has no colour, and `writ`'s own lines do, such as the green tick
beside a tool call that worked.

The status row gives the session, the project, what is held, the share
of the context used, what the session has spent, who ran the last turn
and the model. On the way out `writ` says what is held and how to go on.
`writ --session NAME` goes on, and only one `writ` can have a session
open. While a session runs, stderr goes to `stderr` in its directory. A
session with no prompts leaves nothing behind.

## writ do and writ ask

`writ do [OPTIONS] PROMPT` runs one prompt until the model stops, for
scripts. It takes `--project`, `--session`, `--max-turns` and
`--timeout` as above, and `--thinking` to print the model's thinking
too. The model's text goes to stdout. `[ok]`, `[no]`, `[usage]` and
`[writ]` lines go to stderr, and then what is held and the session's
name. It exits 0 when the model finished, 1 when it was stopped and 2
for an error before it began.

```
writ do --session fix1 'cargo test fails in parse; find out why and fix it'
writ diff fix1
writ apply fix1
writ do --session fix1 'add a test for it'
```

`writ ask [OPTIONS] PROMPT` sends one request to the model in use and
runs nothing. It belongs to no session, so no budget applies. It exits
1 if the turn ended badly, such as at its token limit.

| Flag | Default | |
|------|---------|---|
| `--tool` | off | Offer one tool, `read_file`, and report the call the model makes |
| `--max-tokens N` | 1024 | |
| `--timeout SECS` | 600 | |
| `--record FILE` | | Keep the stream's bytes |
| `--replay FILE` | | Read a recorded stream instead of asking |

## What the model can do

Six tools, each run by a server inside the box. A call to any other name
is refused without reaching it. No path is checked: the box decides what
a path can reach.

| Tool | |
|------|---|
| `read` | A text file, in windows of at most 2,000 lines or 24 KiB. A file over 64 MiB is not read |
| `edit` | Replace one piece of a file, which must appear exactly once. An exact match first, then whole lines, ignoring look-alike characters, then trailing whitespace, then indentation |
| `write` | Create a file, or replace one it has read |
| `grep` | Search with an extended regular expression, skipping `.git`, `target` and `node_modules` |
| `glob` | Find files by name |
| `shell` | Run a command with `/bin/sh` in the project. 120 seconds by default, at most 600. Output past the first and last 8 KiB goes to a file in the box's `/tmp`, and the answer names it |

A file must be read before it is edited or replaced. `oath` is in the
box: it prints its guide there, and `oath get` and `oath ls` work. A
write such as `oath set` fails, as `/oath` is read-only.

At a session's first prompt, the box reads the project's `AGENTS.md`,
or `CLAUDE.md` if there is none, up to 16 KiB, and the model is given it
labelled as notes that grant nothing.

A prompt ends when the model stops calling tools. If it stops without a
word, with its last command failed, or with changes it has run nothing
to check, it is told so, at most twice, and goes on. The prompt is
stopped if the model stops without a word after that, at its turn limit,
at its fourth turn that could not be used (out of tokens, or a malformed
call), at the fifth identical call with no `edit` or `write` that worked,
no `shell` that ran and no old answers masked between them (the model is
warned at the third), when a stream or the box fails, when a content
filter ends a turn, or when the context is 90% full with nothing left to
mask. A `shell` that ran clears the count of identical calls whatever it
exited with, so a repeated `shell` runs to the turn limit.

Old tool answers are masked at the lower of 60% of the context or
150,000 tokens. A turn may use up to 8,192 tokens on a local model, and
up to 32,768 on OpenRouter where its routes allow.

## The box

A box is `/bin/bwrap`, then `writ`'s own second stage. Any failure stops
it before a command runs.

What it has:

- Read-only: `/bin`, `/sbin`, `/lib`, `/oath/store`, `writ`'s own binary
  at `/writ`, and where they exist `/lib32`, `/oath/INDEX.md`,
  `/oath/objects`, `/oath/schema`, `/oath/status`, `/oath/output.json`,
  `/etc/passwd`, `/etc/group`, `/etc/nsswitch.conf`, `/etc/shells`,
  `/etc/hosts` and `/var/empty`.
- An empty 4 GiB tmpfs for home and another for `/tmp`, with `/proc` and
  a small `/dev`.
- `~/.cargo/registry` and `~/.local/share/go/pkg/mod`, where they exist,
  as overlays whose writes vanish with the box.
- The project at its real path, as an overlay: the real tree is the
  read-only layer, and writes land in the session.

What it does not have: a network beyond `lo`, a terminal (stdin is
`/dev/null`, stdout and stderr are pipes), any capability, new user
namespaces, your shell's environment, the rest of `/etc`, `/oath/log`,
`/oath/run`, `/oath/ssh`, `/run`, `/sys`, `/root`, or the rest of home.
`no_new_privs` stops `sudo`. Landlock is on when the kernel has it, and
a seccomp filter always; without Landlock `writ` says so. The box holds
at most 4,096 processes and writes no core files.

Its environment is built from empty: `HOME`, `PATH=/bin:/sbin`,
`TMPDIR`, `TERM=dumb`, `LANG`, and what turns off colour, paging,
prompts and the network for `git`, `cargo` and Go.

A project is a directory under your home, `/tmp` or `/work`, and not one
of those itself. One that holds your home, or sits in a hidden directory
at the top of it, is refused. `~/.cache` is allowed. A project under
`/work` takes `sudo -u work writ`, as `/work` is `work`'s and mode 700,
and that `writ` cannot start a local model, as `sudo` refuses `work`.

In the project, these appear empty: files named `.env` and `.env.*`
(not `.env.example`, `.env.sample`, `.env.template` or `.env.dist`),
`id_rsa`, `id_dsa`, `id_ecdsa`, `id_ed25519`, `.netrc`, `.pgpass`,
`.npmrc`, `.pypirc`, `.git-credentials`, `credentials.json` and
`secrets.json`, files ending `.pem`, `.key`, `.p12` or `.pfx`, and
directories named `.ssh`, `.gnupg` or `.aws`. The search does not go
into `.git`, `target` or `node_modules`. The list is a convenience: a
secret under another name reaches the box, and what the model reads
reaches its provider.

### writ run

`writ run [--project DIR] [--session NAME] [--timeout SECS] -- CMD` runs
one command in a box over the project and exits with its status, or
125 if the box refused to start. `--timeout` kills the box after that
many seconds. Writes are dropped
unless `--session` names where to keep them, in
`~/.local/share/writ/sessions/NAME/upper`. `writ diff`, `writ apply` and
`writ sessions` read only a session `writ` or `writ do` made, which
records its project.

### writ check

`writ check [--json] [-v]` builds a small project and a canary under
`~/.cache/writ`, runs attacks in a box, in the box a server runs in and
in the box curl runs in, and looks at the host to see what they reached.
It tries ordinary work too, and a tool the machine lacks is skipped. It
reads no real secret: for the paths it names it asks only whether they
exist. It exits 1 if an attack got through or work broke. `-v` lists
every result, and `--json` gives the same facts.

```
kernel 7.3.1, Landlock ABI 11, ptrace_scope 1; 29 of 46 sensitive paths exist on this host
485 attacks refused, 9 writes held, 29 kinds of work fine, 0 skipped; 0 got through, 0 broken
```

## Diff, apply, undo

Writes land in `~/.local/share/writ/sessions/<id>/upper`, and only
`apply` takes them to your tree. `writ sessions` lists each session,
what it holds and its project.

```
writ diff fix1             # names, then a diff of each text file
writ diff fix1 --names     # names only
writ apply fix1            # takes what is not flagged
writ apply fix1 build.rs   # takes what you name, flagged or not
writ apply fix1 --all      # takes everything
writ undo fix1             # takes back what apply did
```

```
A build.rs  [runs later]

--- build.rs
@@ -1,0 +1,1 @@
+fn main(){}
```

A letter says what happens to a path: `A` adds, `M` changes, `D`
deletes, `L` makes a symlink, and `?` is a FIFO, socket or device. Text
over 1 MiB and binary files are named and not shown.

`.git` is left out at any depth, so a commit the model makes in the box
stays there. So is what the real tree's git ignores; with no repository,
or no `/bin/git`, anything under `target` or `node_modules`.

A flagged change waits until you name it or say `--all`:

| Flag | |
|------|---|
| `runs later` | `build.rs`, `Cargo.toml`, `Cargo.lock`, `configure`, `Makefile*`, `*.sh`, `*.plan`, `*.pth`, `package.json`, `package-lock.json`, `go.mod`, `go.sum`, `.envrc`, `AGENTS.md`, `CLAUDE.md`, `.mcp.json`, `.gitignore`, `.gitmodules`, `.gitattributes`, `.editorconfig`, `pkg.toml`, `flake.nix`, `Dockerfile`, `Justfile`, `justfile`, `CMakeLists.txt`, `meson.build`, `Rakefile`, `Gemfile`, `setup.py`, `conftest.py`, `pyproject.toml`, `.npmrc`, `.pre-commit-config.yaml`, at any depth, and anything under `.cargo`, `.github`, `.gitlab`, `.claude`, `.vscode`, `.idea`, `.husky` or `.devcontainer` |
| `a symlink` | |
| `its mode changed` | Execute bits, or setuid, setgid or sticky |
| `binary` | Not UTF-8, or a NUL byte in its first 8 KiB |
| `not a file` | A FIFO, socket or device, which is never applied |

Naming a directory takes what is under it. `apply` writes each file
atomically and strips setuid, setgid and sticky. It applies nothing if
anything asked for cannot go: a FIFO, socket or device, a real file
changed after the session began, or a path that goes through something
in your tree that is not a directory. You commit.

`undo` puts back what `apply` changed, newest first, from copies kept in
the session. A file changed after it was applied stays, is named, and
`undo` exits 1. The session keeps its changes. `apply` can take again
what `undo` removed, but a file `undo` put back counts as changed after
the session began, so an `apply` that asks for it applies nothing. A
lock keeps a box and `apply` from running on one session at once.

## Local models

`pkg:llama-cpp` is on the origin and in no profile. It needs a CPU with
AVX2, FMA, F16C and BMI2, and `pkg:gcc` and `pkg:vulkan-loader`, which
`desk` has. A `base` machine adds those two, with `pkg:mpc`, each at the
hash the plan names. A model is a `.gguf` file you fetch yourself, and
it must let others read it and must not let them write it.

```
sudo oath new pkg:llama-cpp --from-json '{"present":true,"hash":"sha256-…","needs":[{"id":"pkg:gcc","hash":"sha256-…"},{"id":"pkg:vulkan-loader","hash":"sha256-…"}]}'
sudo oath apply pkg:llama-cpp
mkdir -p ~/.local/share/writ/models
cp model.gguf ~/.local/share/writ/models/
writ use local             # switch to it, and load it
writ use                   # local: loaded: model.gguf, a 16384-token context, on the CPU
writ llama stop            # free what it holds
```

`writ use local` takes the only `.gguf` in that directory, or
`--model`, and once it has loaded saves what it settled on in
`~/.config/writ/local.json`. A later `writ use local` starts from those
settings, and new ones restart a server that has others. `writ do`,
`writ ask` and each prompt in a session load the chosen model if it is
not loaded. It stays loaded, holding its memory, until `writ llama stop`.
`writ llama start`, `stop` and `status` do it by hand.

`writ serve --model FILE [OPTIONS] [-- ARGS]` is the same server in the
foreground, with its log on the terminal. It saves nothing.

| Option | |
|--------|---|
| `--model FILE` | A name in `~/.local/share/writ/models`, or a path |
| `--threads N` | CPU threads. Default: the server's choice |
| `--context N` | Context size in tokens. For `use`: 32768 with a GPU, or 16384. For `serve`: 16384 |
| `--gpu` | Give the box the GPU, and put the whole model on it. NVIDIA's only |
| `--cpu` | Do not. For `use`, the GPU is the default if the machine has one; for `serve`, the CPU |
| `--gpu-share P` | Hold the server to P percent of the GPU's time, 1 to 100, by stopping it for the rest of each 100 ms. Implies `--gpu`. For `use`: 40 |
| `--mtp`, `--no-mtp` | Draft with the model's own MTP layer, for a file that has one. A file without it does not load. Default: off |
| `-- ARGS` | For `serve`: passed to `llama-server`, in the box |

The server is `/bin/llama-server`. `home` starts it through `sudo -u
work`, and it runs as `work`, in a box of its own: `/bin`, `/lib` and
`/oath/store` read-only, a 256 MiB `/tmp`, the socket's directory, and
no `/etc`, home, `/work` or network. The model comes in through one
read-only descriptor. `--gpu` adds `/dev/nvidiactl` and `/dev/nvidia<N>`.
No port is open: `home` reaches it through a socket in
`~/.local/share/writ/llama`, a directory only `home` can enter, where
its log also is.

## OpenRouter

```
writ key                                    # take a key, unseen
writ use openrouter AUTHOR/NAME --budget 2  # switch to a model
writ use                                    # which model is in use
writ use local                              # back
```

`writ key` reads the key without showing it, or from a pipe, and asks
OpenRouter about it. It keeps nothing OpenRouter refuses, and a line
like `NAME=value` is not a key. It says what is left of the key's limit,
or that the key has none. The key is in
`~/.config/writ/openrouter.key`, mode 600. `writ` reads it for each
request and refuses the file if it is a link, another account's, or
readable by anyone else. It never enters the box.

`writ use openrouter AUTHOR/NAME`, as openrouter.ai/models writes it,
reads OpenRouter's public list of routes when you run it, and at no
other time. It refuses a model with no route that promises to keep
nothing and offers tools, and otherwise says what leaves the machine,
the range of prices and what is left on the key. The model's context is
the smallest among those routes. What it settles on goes in
`~/.config/writ/use.json`. With no name it keeps the model in use, so
`--budget` alone changes a budget.

What the model reads leaves this machine, for OpenRouter and the host
it picks. Every request asks for routes that keep nothing, with no way
to turn that off, and the promise cannot be checked from here.

curl talks to `https://openrouter.ai/api` from a box of its own: `/bin`,
`/lib`, `/oath/store`, `/etc/resolv.conf`, `/etc/hosts`,
`/etc/nsswitch.conf` and a 16 MiB `/tmp`, with no home. Where the kernel
has Landlock, TCP reaches only ports 443 and 53. The key is on curl's
stdin, never in its arguments or environment.

A failed request is tried again at most three times, when it is known
to help: a lost connection, a stream cut off, 408, 429, 500, 502–504,
520–524 or 529, after `Retry-After` up to 300 seconds, or after 2, 5
and 15. What came of a failed try is dropped from the conversation. Any
other failure stops the prompt, with its reason.

### Budget

A session stops before a turn once what it has spent reaches its
budget, which is $1 unless `--budget` says. What it has spent is in the
session's `spent` file, so it holds across a restart. A turn's cost is
known when it ends, so a session can pass its budget by one turn. A
turn that finishes without saying what it cost stops the prompt, and a
turn that fails is not counted. `writ use openrouter --budget DOLLARS`,
or `/use openrouter --budget DOLLARS` in the session, sets a new budget,
and the session goes on. The key's own limit, set at
openrouter.ai/settings/keys, is what stops everything.

## What this does not stop

- A kernel bug. The box shares the kernel, and with `--gpu` that
  includes NVIDIA's driver.
- A change you accept. `writ diff` shows it and cannot judge it.
- Another process running as `home`, outside the box, which can read
  the key file.

## Files

| Path | Holds |
|------|-------|
| `~/.local/share/writ/sessions/<id>/` | `upper`, `transcript.jsonl`, `meta.json`, `undo.jsonl`, `undo/`, `spent`, `stderr` |
| `~/.local/share/writ/models/` | Local models |
| `~/.local/share/writ/llama/` | The local server's sockets and log |
| `~/.config/writ/use.json` | The model in use |
| `~/.config/writ/local.json` | The local model's settings |
| `~/.config/writ/openrouter.key` | The key |
| `~/.cache/writ/` | `writ check`'s project, removed after each run |
