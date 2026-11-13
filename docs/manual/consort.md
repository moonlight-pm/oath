# Consort

Oath machines that know each other (R87, [spec](../specs/consort.md)).
Each member is a **peer**, and every machine runs `svc:oath-bus`, which
finds the others on the LAN, links to the members, and keeps their
facts. No machine leads: any member admits another, at the owner's
word, and any member can be lost.

Built 2026-11-13 (`pkg:oath` with `/bin/oath-bus`); in no release yet.

## What each machine has

| Path | What |
|------|------|
| `/oath/bus/key`, `/oath/bus/link` | This machine's Ed25519 signing key and X25519 link key, made on first start, root's alone (0600, in a 0700 directory) |
| `/oath/bus/log.json` | The membership log: signed entries, outside every generation, so `oath undo` never takes membership back |
| `/oath/run/bus.sock` | The local socket `oath consort` and a desk's view use |
| port 6284 | TCP for links, UDP for the beacon ("OATH" on a phone's keypad) |

Every machine starts as a **consort of one**, with itself as the only
member. Its consort's id is the hash of that first entry.

## Looking

```
oath consort               # the members, machines nearby, asks to join
oath consort --json
oath ls peer               # each member, this machine included (`me`)
oath get peer:aulos        # its key, address, online, who admitted it, its status
```

A `peer` object is inventory, as a `dev` is: `oath-bus` makes one for
each member and removes it when the member leaves. Its actual has the
member's `status`: profile, output, generation, drift, failed services,
boot time, kernel, and what the machine is (`machine`: vendor, model,
CPU). `oath set peer:x present=false` is refused; `oath consort remove`
removes a member.

## Joining

On the machine that joins (a consort of one):

```
sudo oath consort join dulcia          # a name `oath consort` lists nearby, or an address
asked dulcia to admit aulos into consort 3f2a9c1b04d2.
code: 482913
On any member: sudo oath consort admit aulos --code 482913
```

On any member, with the code that machine printed:

```
oath consort                            # the ask, which member holds it, and where from
sudo oath consort admit aulos --code 482913
```

The code is made from the consort's id and both of the joining
machine's keys, so a key swapped on the way shows another code on one
side. `oath consort` on a member never prints it: you type what the
joining machine shows. A wrong code is refused. The new member takes the
consort's log and is linked to every member within seconds.

A machine in a consort with others leaves before it joins another:

```
sudo oath consort leave                 # this machine: a consort of one again
sudo oath consort remove buccina        # any member removes another, for every member
```

A removed key's signatures stop counting from its removal, so it cannot
let itself back in; a later `admit` with the owner's word brings it
back.

## Facts

Each peer publishes facts about itself, signed, to every member: a new
record only when one changes, read every 15 s.

| Topic | What |
|-------|------|
| `status` | profile, output, published, generation, drift, failed services, services running and total, boot time, kernel, machine |
| `services` | each service's state, `enabled`, restarts, since, `last_exit`, `reason` |
| `packages` | each package's pin (12 hex), `present`, `overlay` (as `oath get` says), `source` |
| `load` | load averages, CPUs, memory, the root filesystem's size and free space |
| `processes` | the 15 busiest by CPU and the 15 largest by memory: pid, name, user, CPU %, resident bytes; names only, never command lines |
| `update` | `oath update`'s preview every 30 minutes: the newest output, whether this machine runs it, what would move |

A failed service is one PID 1 gave up on, or one stopped with an error
code or a signal; a one-shot that exited 0 is done.

## The local socket

`/oath/run/bus.sock` takes one JSON request a line and answers one line.
`home` and root may read; only root, or the account `oath-bus` runs as,
changes membership.

| Request | Answer |
|---------|--------|
| `{"op":"state"}` | `me`, `consort` (`id`, `short`, `members`), `nearby`, `asks` (each with its `code`, for a view that shows both codes), `joining` |
| `{"op":"records","peer":"aulos","topic":"status"}` | `records`: `{peer, topic, seq, at, data}`; both filters optional |
| `{"op":"watch"}` | the state, every record, `{"ev":"replayed"}`, then a line per event: `state` or `record` |
| `{"op":"join","to":…}`, `{"op":"admit","name":…,"code":…}`, `{"op":"remove","name":…}`, `{"op":"leave"}` | as the verbs above |

An answer has `ok: true`, or `ok: false` with `error` and `hint`.

## Not yet

Only the LAN: a beacon does not cross a router. Facts only: an update
announced on the bus, requests to another peer (`oath call`), and cells
on a peer are designed, not built ([spec](../specs/consort.md)).
`oath-bus` runs as root.
