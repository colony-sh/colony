> The first-slice work happens in [colony-sh/colony-shell](https://github.com/colony-sh/colony-shell), not in this repository.

You are building the first slice of Colony: one named agent, end to end.

Goal: A named agent on your machine wakes on a schedule, works inside a container, and sends a report with evidence that you can answer from your phone.

The inventory is done. Read docs/first-slice-gaps.md before anything else. If something in it is wrong, say so before building on it.

In scope:
- A daemon (partly built). Runs on one machine and starts at login. It owns the event log, the agents and the heartbeats, so they work with no terminal or app open. Already there: colony-events runs in the background and survives shell sessions. It routes events only, and nothing starts it at login.
- The event log (partly built). Append-only and totally ordered. Every state change is exactly one event. Restarting the daemon replays it to the same state. Already there: Several SQLite stores. None is a single append-only log, and tuples are removed when taken.
- Addresses (not built). An agent, each of its runs, each run's outputs and diff, and the reports viewport all resolve from the command line and from the viewport. Already there: Ids exist as plain strings. There is no colony:// scheme and no resolver.
- One named agent (partly built). A charter, grants, a daily budget, notes it carries between runs, and a state: sleeping, running, parked, paused or retired. Already there: AGENT.md definitions, the @agent syntax, conversation persistence and token budgets. No lifecycle state and no carried notes.
- Heartbeats (mostly built). A schedule, an optional gate command, and the skip rule when a run is still parked. Every beat must be answered: idle, acted, needs you, or missed. Already there: A full scheduler driven by HEARTBEAT.md, with intervals, tool limits, budgets, findings and stored runs. Its agent callback is never set, and it stops with the shell.
- Runs in a container (not built). Each run gets a fresh container with a scratch worktree mounted as an overlay and network access only to granted hosts. Commands go through Colony's shell. Already there: Tool allow-lists, risk scoring and tool approval only. This is the largest piece of new work.
- Reports (partly built). A headline, a status, a body, evidence as addresses, and asks. Idle runs post nothing. Already there: Findings with a severity and a message, and stored runs. No headline, evidence or asks.
- The reports viewport (partly built). A web page served by the daemon, usable on a phone. It shows what needs you first, then the feed, and it is where you answer. Already there: The serve builtin, and an HTML dashboard of the checklist and recent runs. Read-only and not laid out for a phone.
- Ask and answer (partly built). An ask parks the run. Answering in the viewport resumes it, and the log records who answered and when. Already there: Synchronous tool approval and an approvals database. Nothing parks a run or takes an answer from another device.
- Failure handling (partly built). A run that dies or overruns its budget is detected, cleaned up and reported. The next beat fires normally. Already there: Timeouts, cancellation tokens and stale-PID cleanup. No liveness signal for a run.

Out of scope. Do not build these, and do not design for them beyond leaving room:
- Lanes and the shared shell interface
- Claims and versioned writes outside a container's layer
- Other machines over SSH
- Worktree management beyond one scratch worktree per run
- Collections, shapes and components
- Packages and the marketplace
- Rooms, roles and invites
- Isolates
- libghostty and the desktop app. The viewport is a web page.

Decisions already made:
- What runs the agent's turns: Use colony-agent, the loop already in the repository.
- How your phone reaches the daemon: A private network between your devices.
- How you learn an ask is waiting: A push notification from the viewport page.
- Where the log lives: One SQLite file per machine.
- The container runtime: Rootless Podman.
- How the agent's notes are kept: The agent proposes notes and you approve them in the viewport.

Build order. Stop and report after each step:
1. Wire the heartbeat scheduler to colony-agent and see one run complete
2. The event log table and addresses
3. Move the scheduler into the daemon and start the daemon at login
4. The named agent's lifecycle, with a heartbeat per agent
5. Reports with evidence, and the viewport on serve
6. Ask and answer, with parked runs
7. Runs in a container
8. Failure handling and restart

The first agent is the lead miner. It runs once a day, looks for people describing the problem Bellman solves, drafts a reply for each, and reports the drafts. It never sends anything without your answer. It is the first agent because you already want it running, and because its approve-then-send step exercises the whole loop.

Acceptance criteria. The slice is done when all of these have been seen passing:
1. Creating the agent with one command leaves it sleeping, with the time of its next beat shown by agent list.
2. A beat fires with no terminal or app open, and the log shows it.
3. The run's commands execute inside a container. A call to a host that was not granted is blocked, and the block is recorded.
4. Everything a run writes is in its container's layer. Nothing outside the mounted scratch worktree changes.
5. A run that finds nothing to do posts no report and answers idle.
6. A run that acted ends with a report that has a headline, a status and at least one evidence address.
7. Opening an evidence address shows the exact output or diff it names.
8. An ask parks the run, and the next beat does not fire while it is parked.
9. Answering the ask from a phone resumes the run within ten seconds, and the log records who answered and when.
10. Killing the agent mid-run is noticed. The container is removed, the agent returns to sleeping, and the next beat fires normally.
11. A run that exceeds its time or token budget is stopped, answers missed, and posts a blocked report.
12. Restarting the daemon loses nothing: agents, heartbeats and a parked ask all survive.
13. The lead miner runs unattended for seven days in a row, and you answer every ask from the viewport.

Rules:
- Rust only, plus bash for scripts. Ask before adding a language, a service or a large dependency.
- Every state change is exactly one event in the log. Nothing is kept only in memory.
- Nothing leaves the machine, and no host is contacted, without an explicit grant.
- Extend what exists: colony-agent, the heartbeat scheduler, colony-events and serve. Do not write parallel versions.
- Until runs execute in a container, heartbeat runs stay in the scheduler's read-only notify mode.
- Follow the repository's CLAUDE.md. Add new feature ids for this work and never renumber existing ones.
- When something is not covered here, stop and ask. Do not widen the scope.
