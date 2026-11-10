# Oath

You are on **Oath**. Do not assume Debian, Arch, NixOS, Alpine, or Ubuntu.

Do **not** edit `/etc` or random files. Use `oath`.

## Verbs

    oath                 this INDEX (the file is /oath/INDEX.md)
    oath ls              list objects  (optional --kind)
    oath schema [kind]   kinds, or one kind's schema + prose
    oath get <id>        desired + actual
    oath set <id> k=v    write desired only (checked against the kind's type)
    oath new <id> k=v    create a svc, pkg or cell (runs after apply)
    oath rm <id>         remove a svc, pkg or cell (after apply)
    oath diff [id]       drift
    oath apply [id...]   check, snapshot, then converge
    oath keep            keep an apply on probation (from a new SSH session)
    oath undo            put back the spec the last apply changed, and converge
    oath verify          hash linked package trees again against their pins
    oath audit [pkg:x]   the daily security audit's findings for what is here
    oath reap            what is on disk that no package prescribes;
                         `sudo oath reap --confirm` removes it (each boot does)
    oath update          what the newest output for this machine would change;
                         `sudo oath update --confirm [--reboot]` moves to it
    oath log             apply log
    oath logs [id]       output of svc:<name>, net:net0, a plan's last build
                         (plan:<name>), or PID 1 (no id)
    oath fmt <path>      canonical plan.plan
    oath build           isolated plan build (no /bin link; log in /oath/log/plan/)
    oath store pkg:<name>|cell:<name> <dir|tar>
                         put a tree built elsewhere into the store and print
                         its hash; `oath set pkg:<name> hash=…` pins it
    oath mcp             these verbs as an MCP server on stdin/stdout
                         (ssh -T home@<host> oath mcp)
    oath password        set, change or remove home's password
                         (`--status`: is one set, auto-login, the seat)
    oath desk [stop]     give the seat (tty1) back to the desk, or end the
                         desk and give it to the console (a shell as home)

`--json` prints the same facts as JSON. `oath --help` for flags.

## Recipes

These packages are not in the image: their licences do not allow redistribution, or (coding agents) the machine takes the vendor's own build. Each is a plan that fetches the vendor's file and builds the package on this machine.

- `pkg:claude-code` Claude Code: Anthropic's agentic coding tool for the terminal. `sudo oath build plan:claude-code`, then `sudo oath set pkg:claude-code present=true` and `sudo oath apply pkg:claude-code`.
- `pkg:codex` Codex: OpenAI's coding agent for the terminal. `sudo oath build plan:codex`, then `sudo oath set pkg:codex present=true` and `sudo oath apply pkg:codex`.
- `pkg:grok-build` Grok Build: xAI's coding agent for the terminal. `sudo oath build plan:grok-build`, then `sudo oath set pkg:grok-build present=true` and `sudo oath apply pkg:grok-build`.
- `pkg:nvidia` NVIDIA's driver (R34): its closed kernel modules, built on the machine against pkg:linux-build, its GL, EGL, Vulkan, GBM and CUDA libraries (and their 32-bit ones), and nvidia-smi. For a machine with an NVIDIA card: `oath build plan:nvidia`, then present. `sudo oath build plan:nvidia`, then `sudo oath set pkg:nvidia present=true` and `sudo oath apply pkg:nvidia`.
- `pkg:steam` Steam's launcher, in a sandbox with the paths Steam assumes; it installs the client in ~/.local/share/Steam. `sudo oath build plan:steam`, then `sudo oath set pkg:steam present=true` and `sudo oath apply pkg:steam`.

## Kinds here

- `cell` — one project's software, run only in a cell (never linked or published)  (`oath schema cell`)
- `dev` — a hardware device (inventory)  (`oath schema dev`)
- `host` — this machine (hostname, power, env, timezone, mounts)  (`oath schema host`)
- `net` — a network link (net0, dhcp or static)  (`oath schema net`)
- `pkg` — a package (store + /bin links)  (`oath schema pkg`)
- `plan` — a build plan (hash-pinned file; oath build)  (`oath schema plan`)
- `snap` — btrfs generations (apply / undo)  (`oath schema snap`)
- `ssh` — owner SSH public keys (login as home)  (`oath schema ssh`)
- `svc` — a process PID 1 supervises  (`oath schema svc`)

## Safety

`oath apply` checks everything first (a refusal changes nothing), then
takes a filesystem snapshot. Halt, reboot, boot generation changes
(other than `oath undo`) and changes that can cut off SSH need
`--confirm`. Do **not** pass `--confirm` unless the owner asked for that
class of change. A change that can cut off SSH then goes on probation:
PID 1 undoes it after 180 s (`--probation <secs>`), or at the next
boot, unless `sudo oath keep` runs in an SSH session opened after the
apply. Nothing else applies while it is open.

## Where you are

Serial is **root** (break-glass). SSH is **home** (`HOME=/home`);
`sudo` has no password. `home` has none either until `oath password`
sets one (`svc:login` keeps its hash, root's alone): the console asks
for it when `host:local.autologin` is false, and so does a desk's lock
screen; SSH (keys only) and `sudo` never do. The catalog lives at `/oath`. `/home` is the
`home` account's directory. There is no `/usr`; helpers are
`/lib/oath`.

`oath new` makes a `svc` or `pkg`; `oath rm` removes one. Both, like
`set`, only change the spec: nothing happens until `oath apply`, and
`oath undo` takes back the last apply. Changes that can cut off SSH
(sshd, `ssh:local` keys, `net:net0`, a new `pkg:openssh` hash) need
`--confirm`. A service restarts when an apply moves the package its
program comes from.

Updates come from the origins (`host:local.origins`): a build host
publishes each output it checked, signed, and `oath update` takes the
newest for `host:local.profile` when a key in `host:local.publishers`
signed it. It moves every package in one generation, then the kernel,
initrd and PID 1 for the next boot; the boot before stays in the menu.
A package published on its own has a head on the origins
(`pkg/<name>.json`, R72): `oath update` moves each package to the newest
head a key may sign for that name (a `names="…"` list before a key scopes
it), re-pointing a need only when the package still finds every library
it loads. `oath publish pkg:<name>` puts this machine's pin up as its head.
