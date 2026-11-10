# Cells

A cell is one program in a box of its own: namespaces through `crun`, a
network through `pasta`, one uid, no daemon. `cell` is Oath's own program,
in `apps/cell`, on Go's standard library alone. `cell run` stays in the
foreground, so whatever started it supervises it. Its output is the
program's, and so is its exit status (128 plus the signal, if a signal
ended it). Without `-t` that output goes through pipes `cell` makes, so a
program can open `/dev/stderr` by its path even when `cell`'s own output
is root's pipe, as a service's is.

| `cell run` of | Runs |
|---------------|------|
| `<image>` | An image from a registry |
| `pkg:<name>` | A package this machine has installed, with no image |
| `cell:<name>` | One project's own software, the ninth kind |

Nothing on Oath builds a container image (R80). `cell run <image>` takes
what others published; software built on Oath is a `pkg:` or a `cell:`.

## Getting it

`pkg:cell` is on the origin and in no profile. `sudo oath add cell` takes it
at its newest signed head, with each package it needs that the machine
lacks, and applies them at once. It needs `pkg:certs`, `pkg:crun`,
`pkg:passt` and `pkg:zstd`, and `pkg:crun` needs `pkg:json-c`, `pkg:libcap`
and `pkg:libseccomp`. `base` and `desk` hold `certs`, `libcap` and `zstd`, so
five are new:

```
$ oath ls --origin cell
  cell  0.1.0  machines  cell: run a container image over crun and pasta, with one uid and no daemon.
* is on this machine; `sudo oath add <name>` installs one with what it needs
$ sudo oath add cell
Create readonly snapshot of '/oath/run/fs/@' in '/oath/run/fs/@gen-1'
pkg:cell sha256-… (0.1.0), signed by oath@buccina
pkg:crun sha256-… (1.30.1), signed by oath@buccina
pkg:json-c sha256-… (0.19-20260627), signed by oath@buccina
pkg:libseccomp sha256-… (2.6.1), signed by oath@buccina
pkg:passt sha256-… (2026_10_02.cba3570), signed by oath@buccina
added in generation 1
```

Run a second time, it says ``pkg:cell is here at sha256-…; `oath update`
moves it to its head`` and changes nothing. `sudo oath update --confirm`
moves each to its newest head ([Packages](packages.md)). The examples below
run `pkg:python`, which `desk` has and `sudo oath add python` brings to
`base`.

## Commands

| Command | Does |
|---------|------|
| `cell pull <image>` | Fetch an image, or move its tag; prints the digest for linux/amd64 |
| `cell run [flags] <what> [args...]` | Run an image, a `pkg:` or a `cell:`, in the foreground |
| `cell exec [-t] [-w dir] <name> <command...>` | Run a command in a running cell, under the same limits; its exit status is the command's |
| `cell stop <name>` | Ask a running cell to stop, and return at once |
| `cell ls` | Name, `running` or `stopped`, what it runs, when it started (tab-separated) |
| `cell images` | The images on disk, each with its digest |
| `cell rmi <image>` | Forget a name, then do what `cell prune` does |
| `cell prune` | Delete images no name holds, and what crashed cells left |
| `cell login <registry> <user>` | Keep a password, read from stdin, in `auth.json` |

`cell ls` lists a cell while it runs. A run leaves nothing behind when it
ends; `stopped` is what a crash left, which `cell prune` deletes.

## Who runs it

`cell run` refuses root (`cell: not as root: a cell's root is the account
that ran it`). A cell holds one uid, the account that ran it: that uid owns
every file, and is root inside, or the number `--user` or an image's `USER`
names. It cannot change to another uid, so `su` inside fails (`su: can't set
groups: Operation not permitted`).

A server runs as `work`, which `sudo` refuses and which cannot log in;
`home` reaches it with `sudo -u work`. Each account has its own store at
`~/.local/share/cell` (`images/`, `refs/`, `cells/<name>/`, `auth.json`;
`CELL_ROOT` names another directory), so only `sudo -u work cell ls` lists
`work`'s cells.

## A package, with no image

`cell run [flags] pkg:<name> [args...]` runs a package this machine has
installed. Nothing is copied: the cell's root is a directory of links, and
each pack is bound in read-only.

The program is `bin/<name>`, and arguments go to it. With no `bin/<name>`,
the first argument is the program, looked up on `PATH=/bin`. `--entrypoint`
names the program instead, and what follows are its arguments.

```
$ cell run pkg:zstd --version
*** Zstandard CLI (64-bit) v1.5.7, by Yann Collet ***
$ cell run pkg:python
cell: pkg:python has no bin/python, so name a program: pydoc3, pydoc3.15, python3, python3-config, python3.15, python3.15-config
$ cell run pkg:python python3 -c 'print("hello from a cell")'
hello from a cell
```

Flags go before the name; everything after it is the program's.

### What it sees

```
$ cell run --entrypoint /bin/ls pkg:python -C / /etc /oath/store/pkg
/:
bin  dev  etc  lib  oath  proc	root  run  sys	tmp  var

/etc:
group  hostname  hosts	passwd	resolv.conf

/oath/store/pkg:
bash   coreutils  glibc  libffi   openssl  xz	 zstd
bzip2  curl	  gmp	 mbedtls  python   zlib
```

- **The packs.** The package, each `--with`, a floor of `glibc`, `bash`,
  `coreutils` and `curl`, and what each of those needs, all the way down
  (`curl` brings `mbedtls` and `zlib`, `coreutils` brings `gmp`). Each is at
  the hash this machine links, read from `oath --json get`. Each is bound
  read-only, `nosuid` and `nodev`, at `/oath/store/pkg/<name>/<hash>`, with
  a `live` link beside it. `curl` is on the floor because `pkg:openssl`'s
  default CA path (`ssl/cert.pem`) is a link to `pkg:curl`'s bundle and its
  plan does not name `pkg:curl`.
- **What the floor leaves out.** `grep` and the rest of a machine's tools: a
  script that calls one fails (`/bin/sh: line 1: grep: command not found`)
  until its package needs it or `--with pkg:grep` brings it.
- **`/bin` and `/lib`.** The cell's own links, made by a machine's rules
  (`bin/`, and `lib/` less `oath`, `modules`, `firmware` and `pkgconfig`).
  The first pack to name an entry has it, and the package comes first.
- **`/etc`.** `passwd` and `group`, naming root and any number `--user`
  gives, and `hosts`, `hostname` and `resolv.conf`. Nothing of the
  machine's. The environment is `PATH=/bin` and `HOME=/root` unless `-e`
  says else.
- **Not there.** The machine's `/etc`, its catalog, `/home`, `/work` and
  every other package.
- **What it writes.** Its own root, `/bin` and `/tmp` included, which goes
  when it stops. A write into a pack fails (`Read-only file system`). What
  should last goes to a `-v` volume.

## Flags

`cell run` takes these for all three. `-p`, `--reach`, `-v`, `--tmpfs`,
`-e`, `--env-file` and `--with` may repeat.

| Flag | Does |
|------|------|
| `--name <name>` | The cell's name, `[a-z0-9][a-z0-9_.-]*`; two cannot share one. Without it, `run-<pid>`. |
| `-p [addr:]host:port[/udp]` | Publish the cell's `port` on this machine's `host`. On `127.0.0.1` unless `addr` says else; TCP unless `/udp`. |
| `--reach port[:hostport]` | The cell's `127.0.0.1:port` reaches this machine's loopback `hostport`, TCP. Default `hostport` is `port`. |
| `-v /host/path:/path[:ro]` | Bind a file or directory. `/host/path` must exist. Read-write unless `:ro`. |
| `--tmpfs /path` | A memory file system, mode 1777. |
| `-e NAME=value` | Set the environment. Every process listing shows it. |
| `--env-file <file>` | `NAME=value` lines; blank and `#` lines are left out; a value is taken as it stands, quotes and all. `-e` wins over it. |
| `--with pkg:<name>` | Another package, for a `pkg:` or `cell:` run. An image run refuses it. A bare name is taken as `pkg:`; a `cell:` is not a package. |
| `-w <dir>` | The directory to start in; absolute. |
| `--entrypoint <program>` | The program, over the image's or `bin/<name>`. |
| `--user uid[:gid]` | The number the cell's one uid wears (below). |
| `--read-only` | The root is read-only; `/tmp` needs `--tmpfs /tmp`. |
| `--no-net` | No way out: only `-p` and `--reach`, and `-p` answers only over this machine's loopback, whatever `addr` says. Names do not resolve. |
| `-t` | A terminal (`TERM=xterm`). |
| `--stop-after <duration>` | How long a stop waits before SIGKILL; default `10s` (`2s`, `1m`). |

## A server

```
sudo mkdir -p /work/web
sudo sh -c 'echo "hello from the volume" >/work/web/index.html'
sudo chown -R work:work /work/web
sudo -u work cell run --name web -p 8080:8080 -v /work/web:/data -w /data pkg:python python3 -m http.server 8080
```

From another shell:

```
$ curl -fsS http://127.0.0.1:8080/index.html
hello from the volume
$ sudo -u work cell ls
web	running	pkg:python	2026-11-09T23:55:48Z
$ sudo -u work cell exec web ls -la /data
total 4
drwxr-xr-x 1 root root 20 Nov  9 23:55 .
drwxr-xr-x 1 root root 80 Nov  9 23:55 ..
-rw-r--r-- 1 root root 22 Nov  9 23:55 index.html
$ sudo -u work cell stop web
```

`cell exec` takes `-t` and `-w` before the name. Files it and the program
write are `work`'s outside and root's inside.

**Stopping.** `cell stop` sends SIGTERM to `cell` and returns at once.
`cell` hands the program its image's `STOPSIGNAL`, or SIGTERM, and sends
SIGKILL `--stop-after` later; SIGINT, SIGHUP, SIGQUIT, SIGUSR1 and SIGUSR2
pass through as they are. `cell run` returns when the program has ended,
with its status: 137 if SIGKILL ended it.

**The network.** A cell's network is its own. What comes in is what `-p`
names. Going out, it reaches what its account reaches, other machines
included, and not this machine's loopback: `--reach 9000` opens the machine's
`127.0.0.1:9000` at the cell's own, and `--reach 9100:9000` at the cell's
`9100`. One cell talks to another through a published port and `--reach`;
there is no bridge between cells. Names resolve through `169.254.1.1`,
where `pasta` answers from the host's. `pasta` makes that network through
`/dev/net/tun`: PID 1 loads `tun` at boot and opens the node to every
account. A `--no-net` cell does not use it.

**Limits.** The first process has `no_new_privs`, and uid 0 inside keeps
only the default capability set; any other number has none. seccomp refuses
new namespaces, `mount`, `bpf`, `ptrace` and `io_uring`, among others.
There is no memory or CPU limit: `CONFIG_MEMCG` and `CONFIG_CFS_BANDWIDTH`
are off in Oath's kernel.

## `--user`

`--user uid[:gid]` is the number the cell's one uid wears; a lone uid is its
own group too, unless an image's `/etc/passwd` gives that uid another. For
software that refuses root:

```
$ cell run --user 70:70 --entrypoint /bin/id pkg:python
uid=70(cell) gid=70(cell) groups=70(cell)
```

A `pkg:` or `cell:` cell's `/etc/passwd` and `/etc/group` name that number
`cell`, for a program that looks itself up, as PostgreSQL does. `root` is
the only other name there (`--user bob` is refused: `user "bob": not in the
image's etc/passwd`). On an image, `--user` is over the image's `USER`, and
may name an account in the image's own `/etc/passwd`.

## Under a service

A `svc` runs `cell` like any program, as `work`, and its output is the
service's log:

```
sudo oath new svc:web --from-json '{"exec":["/bin/cell","run","--name","web","-p","8081:8080","-v","/work/web:/data","-w","/data","pkg:python","python3","-m","http.server","8080"],"wants":[],"restart":"always","enabled":true,"user":"work"}'
sudo oath apply svc:web
oath get svc:web --actual
oath logs svc:web
```

With `restart: always`, `cell stop` ends the program and PID 1 starts it
again. A service stops by `oath set svc:web enabled=false` and `oath
apply`.

## `cell:<name>`, one project's own software

`cell:<name>` is a kind beside `pkg`, for software that belongs to one
project. Its trees are under `/oath/store/cell/<name>/<hash>/`, apart from
every package's. It is not linked into `/bin` or `/lib`, it comes from no
origin, and `oath publish` refuses it: `cell:hamlet is a cell, and a cell
never goes to an origin (R78)`. Its name does not clash with a package's:
`cell:python` and `pkg:python` stand side by side. Fields are in `oath
schema cell`: `present` (default `true`), `hash` (`sha256-` plus 64
lowercase hex, required while present), `needs`, and `description`, `home`
and `source`, which are as for a `pkg` and in no hash. An apply starts
nothing, whatever `oath new` prints: a `svc`, or whatever runs the project,
runs `cell run` ([Under a service](#under-a-service)).

```
$ sudo oath store cell:hamlet ./hamlet
cell:hamlet sha256-…
$ sudo oath new cell:hamlet hash=sha256-… 'needs=["pkg:python"]'
cell:hamlet created; `oath apply cell:hamlet` starts it
$ sudo oath apply cell:hamlet
Create readonly snapshot of '/oath/run/fs/@' in '/oath/run/fs/@gen-2'
applied generation 2 (cell:hamlet)
$ oath get cell:hamlet
cell:hamlet  status=in-sync  safety=mutate
desired:
  hash: "sha256-…"
  needs: ["pkg:python"]
  present: true
actual:
  hash: "sha256-…"
  needs: ["pkg:python"]
  present: true
  verified: {"at":"2026-11-09T23:57:20Z","ok":true}
$ sudo -u work cell run --name hamlet -p 8080:8080 -v /work/hamlet:/data cell:hamlet
```

`oath store` takes a directory or a tar of one, prints the hash and changes
no object. `--json` gives `{"hash":"sha256-…","id":"cell:hamlet"}`.

`cell run cell:<name>` is the same box as a package's, with the tree bound
read-only at `/oath/store/cell/<name>/<hash>` and a `live` link beside it.
Its program is `bin/<name>` in the tree, linked into the cell's own `/bin`,
unless the command line names another; it takes the flags a package's run
takes. `--with` adds a package, never a `cell:`.

`needs` are package ids with no hash: a cell starts with each at the hash
this machine links, so an update that moves a package leaves nothing to
re-point. `sudo oath add <name>` brings a need this machine lacks.
`oath update` moves a package a present cell needs as it moves any other,
and never takes one off the machine.

`sudo oath set cell:hamlet hash=sha256-…` and `oath apply` move the pin.
`present=false` keeps the tree and takes `live` away, and `cell run` then
refuses it. `oath rm` and `oath apply` delete the object and leave the
trees for reap; `oath undo` brings the object back.

`oath verify cell:hamlet` hashes the tree again, and PID 1's check once a
boot (`oath verify --quiet`) hashes every present cell's tree beside the
packages'. A tree that differs prints `cell:hamlet DAMAGED: sha256-… hashes
to sha256-…`, and `oath verify` exits 1. `cell run` starts what the last
apply linked, without hashing it.

### Refusals

| When | Condition | Says |
|------|-----------|------|
| `oath new`, `oath set` | `present` with no `hash` | `cell:x: present needs a hash` |
| | a need that is not `pkg:<name>` | ``cell:x: needs: `python` is not pkg:<name>`` |
| | a need with a hash | `cell:x: invalid type: map, expected a string` |
| `oath apply` | the hash is not in the store | `cell:x sha256-… is not in the store`, with a hint to `oath store` it |
| | the tree does not hash to the pin | `security: cell:x hashes to sha256-…, not its pin sha256-…`, exit 4 |
| | a need this machine lacks | `cell:x needs pkg:nosuch, which this machine would not have`, hint `oath new pkg:nosuch, or take it out of cell:x's needs` |
| | a package a present cell needs is being removed | The same, naming the cell, on any apply that has something to change, until the cell does not need it |
| `oath publish` | always | `cell:x is a cell, and a cell never goes to an origin (R78)` |
| `oath add` | a `cell:` id | `cell:x is not a package`, hint `oath add <name>` |
| `cell run` | `present: false`, or no such object | `cell:x is not installed on this machine`, or `no object cell:x` |

### From a plan

A plan makes a cell with `"produces": "cell:<name>"` and builds from
`cell:<name>-src`, so a project's source stays off every origin too:

```
#!oath-plan
{
  "name": "hamlet",
  "produces": "cell:hamlet",
  "build_needs": [
    {"id": "cell:hamlet-src", "hash": "sha256-…"},
    {"id": "pkg:bash", "hash": "sha256-…"},
    {"id": "pkg:coreutils", "hash": "sha256-…"},
    {"id": "pkg:glibc", "hash": "sha256-…"},
    {"id": "pkg:gmp", "hash": "sha256-…"}
  ],
  "run_needs": [
    {"id": "pkg:python", "hash": "sha256-…"}
  ]
}
---
#!/bin/sh
set -eu
mkdir -p /out/bin
cp /oath/store/cell/hamlet-src/sha256-…/hamlet /out/bin/hamlet
chmod 0755 /out/bin/hamlet
```

```
sudo oath store cell:hamlet-src ./src
oath fmt -w hamlet.plan
sudo oath set plan:hamlet --from-file hamlet.plan
sudo oath build plan:hamlet               # cell:hamlet sha256-…
sudo oath set cell:hamlet present=true
sudo oath apply cell:hamlet
```

`cell:<name>-src` is in the store alone, with no object. `oath build` makes
the `cell:` object if it is missing, with `present: false`, the product as
its `hash`, and `run_needs` as its `needs`, less their hashes. A build that
prints `(cached)` makes none; after `oath rm` of the cell, `oath new` makes
it again. An object that exists keeps its pin; `oath set cell:hamlet hash=…`
moves it. A build need the store lacks stops the build: `cell:hamlet-src @
sha256-… is not in the store`.

`produces` is `pkg:<name>` or `cell:<name>`, matching the plan's name. Only
a plan that produces a cell builds from a `cell:`: anywhere else `oath fmt`
and `oath set` say ``need id must be pkg:<name>, got `cell:hamlet-src` ``.
`run_needs` are packages in every plan.

### What goes

`oath reap` holds a cell's versions as it holds a package's, under
`cell:<name>`: it keeps the pin, what a kept generation pins, a tree that
arrived in the last day (`--arriving` for another span) and `cell:<name>-src`
while a filed plan pins it; the rest goes with `sudo oath reap --confirm`.

## An image

`cell run [flags] <image> [args...]` pulls the image if it is not on disk,
and runs a copy of it, a reflink on btrfs. The image's `Entrypoint` and
`Cmd` are the program; arguments replace `Cmd`, and `--entrypoint` replaces
the program. A bare name is Docker Hub's.

```
$ cell pull alpine:3.20
cell: docker.io/library/alpine:3.20: layer 1 of 1, 3 MB
sha256:c64c687cbea9300178b30c95835354e34c4e4febc4badfe27102879de0483b5e
$ cell images
docker.io/library/alpine:3.20	sha256:c64c687cbea9300178b30c95835354e34c4e4febc4badfe27102879de0483b5e
$ cell run alpine:3.20 echo hello from an image
hello from an image
```

```
sudo -u work cell run --name nginx -p 8090:8080 nginxinc/nginx-unprivileged:alpine
```

A tag moves only on `cell pull`; a name with `@sha256:…` holds one image
for good. Only linux/amd64 is taken. A manifest named by digest, and every
blob and layer, is checked against its digest. A layer's own `urls` are
never fetched. Layers are gzip, zstd or plain tar. Setuid bits, devices and
xattrs in an image do not land. A registry that is not this machine is
HTTPS.

A program that changes uid fails, because the cell has one uid: stock
`nginx:alpine` hands its cache to its `nginx` user (101) and stops at
`nginx: [emerg] chown("/var/cache/nginx/client_temp", 101) failed (22: Invalid argument)`,
which is why the server above is `nginx-unprivileged`. An image whose start
script drops to its own account when started as root runs when `--user`
names that account (`redis:alpine` with `--user redis`). An image's `USER`
and `STOPSIGNAL` apply.

## Known defects

- A program that is first in its box and has no handler for SIGTERM ignores
  it, so `cell stop` waits out `--stop-after` for SIGKILL. `python3 -m
  http.server` is one, and exits 137.
- Stopping a service signals every process in its cgroup, and the box goes
  within the second: a SIGTERM handler that takes time does not finish.
  With `"keep_children": true` on the `svc`, PID 1 signals only `cell`,
  which hands the signal on; a handler then has 5 s, or `--stop-after` if
  that is shorter, and when it ends `cell` removes its root. At 5 s PID 1
  kills `cell`: a handler still running does not finish, and a `stopped`
  cell is left for `cell prune`.
- The hint on a damaged tree is wrong: `oath store cell:<name> <dir>` of the
  same files leaves the damaged tree where it is. Delete
  `/oath/store/cell/<name>/<hash>` first, then store.
- `oath rm` of a package a present cell needs stays pending, and every
  `oath apply` with something to change stops at it, whichever object it
  names. `oath undo` leaves the mark in place. Taking the package out of the cell's `needs` lets the
  next apply finish the removal.
