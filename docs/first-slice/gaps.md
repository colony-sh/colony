> The first-slice work happens in [colony-sh/colony-shell](https://github.com/colony-sh/colony-shell). This inventory describes that repository, not this one.

# First-slice gap list

Inventory of this repository against the Colony first slice: one named agent, end to end.
Taken 2026-11-11 by reading the code. Nothing was built or run, and not every line was read:
statuses come from module docs, public types, builtins and call sites.

## State of the repository

- Version 0.4.2, branch `main`, last commit `5302aac` (F081, canvas and draw writes via the Bellman door).
- Uncommitted: `.gitignore` and `CLAUDE.md` modified, `.claude/settings.local.json` untracked.
- 39 crates, about 131,000 lines of Rust, about 2,170 Rust tests.
- `features.json`: 73 features. 58 complete, 2 in progress (F055 recipes, F075 context), 10 new, 1 paused (F054 cloud), 2 deprecated.
- Bash compatibility: 2,447 BATS tests in 56 files. The last full run in `history.txt` (2026-10-30) had 76 failures on `main`, and one coproc test (`37_coproc`, "coproc-3a") hangs without a timeout. `[[ ]]`, `(( ))`, extglob, coproc, mapfile and shopt are all implemented.

## What exists, by area

| Area | Where | What it does |
| --- | --- | --- |
| Shell | `colony-parser`, `colony-expand`, `colony-exec`, `colony-shell` | Pest grammar, expansion, execution, job control, builtins, REPL and TUI REPL, `-c` and script modes. |
| Tuple space | `colony-tuple` | Linda primitives (`out`, `in`, `rd`, `inp`, `rdp`), in-process and cross-process, SQLite persistence, TTL, subscriptions, transactions. |
| I/O channels | `colony-channels` | fd 3 context, fd 4 LLM history, fd 5 tool calls, fd 6 diagnostics, with a framed protocol and pipe redirects. |
| Agent loop | `colony-agent` | Anthropic, OpenAI and Ollama providers with fallback, a tool registry (`shell_exec`, `file_read`, `file_write`, `search`), tool approval, conversation persistence in SQLite, token budgets, cancellation, planning. Uses the `claude-sdk` crate. |
| Named agents | `colony-exec/src/special_files/agent_md.rs`, F059 | `AGENT.md` definitions (description, capabilities, tools, parameters, prompt, hooks) and `@agent "prompt"` syntax. |
| Heartbeats | `colony-exec/src/heartbeat/` | A scheduler driven by `HEARTBEAT.md`: interval with hot reload, a security mode that limits tools, timeout, tool-call cap, token budget, findings with severity, runs stored in SQLite, tuples emitted, a `heartbeat` builtin, a TUI panel and an HTML dashboard renderer. |
| Background daemon | `colony-events`, `colony-exec/src/events/` | An event router that polls the persistent tuple space by rowid, matches listeners registered with `on <pattern> <handler>`, spawns handlers with a timeout, logs runs, keeps a PID file. |
| HTTP server | `colony-exec/src/serve/` | The `serve` builtin: an axum server with static files, shell-function endpoints, webhooks with HMAC, Rack and CGI. |
| Approvals and secrets | `colony-agent/src/approval.rs`, `colony-secure` | Tool approval through an OSC widget with a stderr fallback; encrypted secure variables with keychain, an approvals database and process lineage. |
| History and observation | `colony-history`, `colony-reflection` | Command history in SQLite with full-text search, agent attribution and risk scoring; an observations database. |
| MCP | `colony-mcp` | Connection pool, TypeScript client codegen, an executor session with approval over the tuple space. |
| Memory and context | `colony-memory`, `colony-context` | Vector memory with decay; context composition and budgets. |
| Interface | `colony-tui`, `colony-ui-core`, `colony-panel`, thirteen `*-bin` panels, `widget_server.rs` | Reactive TUI, programmable prompt, panel protocol, widget server over a Unix socket. |
| Bellman | `colony-exec/src/bellman_door.rs`, `canvas_builtin.rs`, `draw_builtin.rs` | Reads and writes a Bellman room's canvas through the local bridge door. |
| Other | `colony-debug`, `colony-recipe`, `colony-recommend`, `colony-cloud-sdk` | Script debugger and X-Ray, recipes, command recommendations, the paused cloud SDK. |

## Mapping to the first slice

| In-scope item | Status | What exists | What is missing |
| --- | --- | --- | --- |
| A daemon | Partial | `colony-events` already runs in the background, survives shell sessions and talks to shells through SQLite. | It routes events only. It does not own agents or heartbeats, and nothing starts it at login (no launchd or systemd unit). |
| The event log | Partial | Several SQLite stores: the persistent tuple space, the reflection database (observations, listeners, `heartbeat_runs`, `event_handler_runs`), command history, agent conversations and secure-variable approvals. The router already orders tuples by rowid. | No single append-only log. Tuples are removed by `in`, so the tuple table cannot be the log as it stands. |
| Addresses | Nothing | Run ids, history ids and session ids exist as plain strings. | No `colony://` scheme, no resolver. |
| One named agent | Partial | `AGENT.md` definitions, `Agent::from_definition`, conversation persistence, token budgets. | No lifecycle state (sleeping, running, parked, paused, retired), no notes carried between runs with approval, no daily budget. |
| Heartbeats | Partial, close | The whole scheduler described above. | The agent callback is never set, so a run cannot reach an agent (see below). The scheduler lives inside a shell process and stops with it. One checklist per directory tree, not per named agent. No gate, no parked-run skip, no cron. |
| Runs in a container | Nothing | Tool allow-lists by security mode, risk scoring, tool approval. | No container, sandbox, overlay or network restriction anywhere. This is the largest piece of new work. |
| Reports | Partial | `Finding` (severity and message) and `HeartbeatRun` (status, findings, raw output), stored and emitted as tuples. | No headline, no evidence addresses, no asks. |
| The reports viewport | Partial | `serve` can host it, and `heartbeat render` already produces an HTML dashboard of the checklist and recent runs. | Not served by default, not laid out for a phone, read-only. |
| Ask and answer | Partial | Synchronous tool approval; an approvals database in `colony-secure`; approval over the tuple space in the MCP session. | Nothing parks a run and resumes it later, and nothing accepts an answer from another device. |
| Failure handling | Partial | Heartbeat `Timeout` and `Error` statuses, cancellation tokens, handler timeouts and stale-PID cleanup in the router. | No liveness signal for a run, no cleanup of a dead run's resources, no restart test. |

## What this changes in the spec

1. **Agent harness.** The spec offered wrapping an outside agent tool or writing a new loop. Neither is needed: `colony-agent` is already a loop with tools, approval, budgets and three providers. Use it.
2. **Heartbeats.** Extend the existing scheduler. Do not write a new one. The first job is wiring the callback.
3. **Daemon.** Grow `colony-events` into the daemon, or add a sibling that shares its pattern (SQLite as the channel, PID file, polling by rowid). Move the heartbeat scheduler out of the shell process and into it.
4. **Event log.** Add one append-only table beside the tuple table in the existing database. Do not add a new store.
5. **Viewport.** Start from `serve` and the heartbeat dashboard.
6. **Reports.** Extend `Finding` and `HeartbeatRun` with a headline, evidence and asks.
7. **Ask and answer.** The approvals database in `colony-secure` is a candidate for persisting asks.
8. **Containers.** Entirely new. Budget the most time here.

## Noticed along the way

- `HeartbeatScheduler::set_agent_callback` is defined in `colony-exec/src/heartbeat/mod.rs` and has no caller anywhere in the workspace. As the code reads, `heartbeat run` returns "no agent callback configured". This was not confirmed by running it.
- Feature ids collide. Code comments call the heartbeat scheduler F080 and the event router F081. `features.json` gives F080 and F081 to the Bellman door work. The heartbeat scheduler, the event router daemon and the `serve` builtin have no entries in `features.json`.
- `rules.txt` is stale in places. Rule 40 rules out `[[ ]]` and `(( ))`, and the grammar implements both. Rule 1 rules out TUI widgets, and rules 55 to 59 describe the TUI that exists.
- F076 "Personal App Platform" is still `new`, with a design document in `docs/plans/`. It overlaps the app-platform direction and predates it.
- The only git remote is named `legacy`.

## Corrections

Found in colony-shell during build step 1, after this inventory was taken. The text above has not been rewritten to match.

- `colony-agent`'s `process_full` runs one model turn. It is not a multi-turn tool loop.
- The Ollama provider sends no tools.
- `shell_exec` runs commands with `sh -c`, not through Colony's shell.
- `token_budget` is an estimate at 4 characters per token, not a real token count.
