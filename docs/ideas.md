# Ideas

Research worth keeping that nothing has acted on. Nothing here is decided or
built; what is decided goes in [decisions.md](decisions.md).

## A coding agent on a cheap model, through OpenRouter (parked 2026-11-03)

The goal was an agent that runs DeepSeek V4.1 Flash through OpenRouter, with
nothing downloaded or sent home behind the owner's back. Joshua parked it to
rethink it. `pkg:opencode` and a `codex-openrouter` wrapper were tried and left
out. Everything below was read or run on 2026-11-02, with Codex at 0.164.0;
prices and versions will have moved.

### The agents

Read from their releases, not run, except OpenCode and Codex.

| Agent | Tree | Release check | What it does on its own |
|---|---|---|---|
| Codex 0.164.0 | static binary | OpenAI's Sigstore signature | See below; every call but one has a switch |
| OpenCode 1.19.4 | Bun executable | GitHub attestation only | First start runs an npm install of `@opencode-ai/plugin` and about 30 packages, with no switch. First search downloads `rg` unchecked. Language servers can fetch code unless LSP downloads are off |
| Pi 1.4.0 | 120 npm packages in a Bun binary | `SHA256SUMS` only | Downloads the newest `rg` and `fd` unchecked unless `PI_OFFLINE=1`. Telemetry on until `PI_TELEMETRY=0`. No permission system, by design |
| Crush | Go, 247 modules | Sigstore-signed checksums | Not looked at further; GitHub cannot identify its licence |
| Goose | 1,128 crates | none | Not looked at further |
| Forge | 906 crates | none | Not looked at further |
| deepseekcode | Go, 195 modules | checksums only | Not looked at further; 87 stars |
| Aider | Python, 105 pinned | no binaries | Last release August 2025 |

Claude Code was dropped for this: OpenRouter only guarantees it with Anthropic as
the provider. After what Oath ships, R57's next route is our own small agent in
`apps/`, about the size of `cell` (2,900 lines): a session or more of work, with the
permission logic ours to own.

### Codex through OpenRouter

Works over the Responses API: a custom provider with `wire_api = "responses"`,
`base_url = "https://openrouter.ai/api/v1"` and `env_key`. A logging CONNECT proxy
showed what it does by default:

| | Defaults | Switches off |
|---|---|---|
| Hosts | `openrouter.ai`, `ab.chatgpt.com`, `chatgpt.com`, `api.github.com`, `codeload.github.com` | `openrouter.ai` |
| Sent to OpenAI | 579 KB to `ab.chatgpt.com` (contents unknown) | nothing seen |
| Code downloaded | 30 MB plugin bundle with native binaries and MCP servers | none |
| Key visible to commands | yes | no |

- **The switches:** `analytics.enabled`, `feedback.enabled` and `web_search` off;
  `shell_environment_policy.exclude = ["OPENROUTER_*"]`; and these `[features]` off:
  `apps`, `browser_use`, `browser_use_external`, `browser_use_full_cdp_access`,
  `computer_use`, `daemon_auto_start`, `hooks`, `image_generation`, `in_app_browser`,
  `in_app_dictation`, `in_app_updates`, `in_app_voice`, `plugin_sharing`, `plugins`,
  `realtime_conversation`, `remote_plugin`, `skill_mcp_dependency_install`,
  `tool_suggest`, `workspace_dependencies`.
- **The one call with no switch found:** the TUI fetches `announcement_tip.toml` from
  `raw.githubusercontent.com`. It is display text; `show_tooltips = false` did not stop it.
- **Not finished:** the last proxy logs, after the trust overrides went in, came back
  empty. Check the host list again before relying on it, and on every Codex bump,
  since a new version can add calls.
- **Metadata:** Codex does not know this model, so `model_context_window` is set by hand.
- `pkg:codex` stays as it ships, so running `/bin/codex` directly has all the defaults.

### Keeping it away from the rest of `home`

Codex's sandbox stops writes and the network, not reads: a command could read any
file `home` can. Running as `work` (R55) does not fit, since `/home` is 0700 and a
project under it cannot be reached without moving it.

What was tried is a bubblewrap wrapper around Codex, nested over Codex's own sandbox:

- **Mounts:** read-only `/bin`, `/sbin`, `/lib`, `/etc`, `/oath` (with tmpfs over
  `/oath/run` and `/oath/log`); the project and a state directory read-write;
  nothing else from `$HOME`. A worktree's git directory read-only.
- **Separate:** `CODEX_HOME` and `HOME` in the state directory, so it cannot see `~/.codex`.
- **The key:** read from one named line of a key file, passed over a pipe on FD 3, so it is
  in no environment but Codex's.
- **`--new-session`:** blocks TIOCSTI (below).

Tested on a toy project with real Codex:

- **An ordinary command** could write the project and run `git status`. It could not write
  `.git`, `/etc`, `/oath` or its config; read the key file, `~/.ssh`, `~/.codex`, `~/.claude`,
  other projects, `/work` or `/root`; use `sudo`, `/dev/kvm`, `/dev/uinput` or PID 1's socket;
  reach the network; or type into the terminal.
- **A command approved outside Codex's sandbox** still had no key file, `sudo` or TIOCSTI, but
  could write `.git/hooks` and read the key from Codex's `/proc` environ.
- **An untrusted project:** `AGENTS.md` does not reach the model, and a "Folder access"
  dialog shows at each start.
- **A trusted project:** `AGENTS.md` works, but a repository's `.codex/config.toml` can start
  an MCP server, which can read the key through `/proc`. Command-line overrides held. Trust
  only your own repositories.

The structure Joshua asked for: a vendor package stays pure, anything Oath packages is
generic and takes options, and one person's model or account lives in a script of theirs.

### OpenRouter and zero data retention

- **Model:** `deepseek/deepseek-v4.1-flash-1017` matches DeepSeek's current build. The plain
  slug's endpoints are the older `20260910` build.
- **ZDR routes:** 37 endpoints across 28 providers. DeepSeek's own API, Alibaba and Baidu are
  not among them. $0.025/$0.25 to $0.45/$1.80 per million tokens in/out; the cheapest are flex
  or lower-precision `fp4`, and DeepInfra `fp8` was $0.098/$0.294.
- **Enforcing it:** Codex cannot send per-request routing, so it has to be on the account or the
  key. A guardrail with `enforce_zdr_other: true` (`POST /guardrails`, then
  `POST /guardrails/{id}/assignments/keys`) binds one key. Tested: DeepSeek, Alibaba and Baidu
  refused with 404, DeepInfra served. It covers models outside Anthropic, OpenAI, Google and xAI.
- **Limits:** ZDR is a policy promise that cannot be verified, and OpenRouter excepts CSAM review
  and in-memory prompt caching.
- **Management keys** cannot call models but can mint keys, so one in a file `home` can read is
  readable by every agent running as `home`.

### Found on the way: TIOCSTI

`dev.tty.legacy_tiocsti=1` on this kernel, and `pkgs/linux/linux.fragment` does not mention it.
A control test on a throwaway pty injected input with no sandbox, and so did
`sudo -n -u work` running `TIOCSTI`. So a process started interactively with `sudo -u work`
can type into `home`'s shell, which undercuts R55. Candidates: `CONFIG_LEGACY_TIOCSTI` off, or
the sysctl at boot. Not decided or fixed.
