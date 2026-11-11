# Colony decisions

Recorded 2026-11-10 from the design session. Each line is a decision the owner made or confirmed.

## Name and foundation
- The name is Colony, reviving the existing project. The command is `colony` and addresses are `colony://`.
- It is built on the existing Colony shell, which runs, is bash compatible and has the tuple space.
- Rust, plus bash. libghostty for programs that emit terminal escape sequences.
- Bash is the language. Extensions are builtins such as `claim`, `wait`, `agent`, `heartbeat`, `container`, not new syntax.

## Shell
- One session is shared by a person and agents. Each actor has its own lane.
- Control is concurrent, not turn-based.
- File writes are optimistic and carry a base version. Claims with leases cover only what cannot be merged.
- A person preempts an agent. An agent never preempts a person.
- Every state change is one event in a totally ordered log per session.

## Environment
- Everything has an address: sessions, lanes, jobs, outputs, claims, events, worktrees, agents, runs, viewports, shapes, records, packages, rooms, containers.
- Each machine runs a daemon that owns its sessions, claims and logs. Other machines attach over SSH.
- Worktree management is built in. An agent lane starts in its own worktree, and merging back needs approval.
- Containers are first class: the sandbox for programs Colony does not control. Writes land in a layer that is committed or discarded.
- Isolates are the sandbox for code Colony hosts: component logic, package scripts, record triggers, heartbeat gates. They reach out only through bindings.

## Agents
- Heartbeats are first class and mean scheduled wake-ups for agents. The liveness signal is a separate thing called a pulse.
- Named persistent agents carry out tasks on their own. They persist as a charter, notes and a log, with a fresh context each run.
- Agents send reports to an addressable viewport. Reports carry evidence as addresses, and asks that park the run.
- Agents can add UI components.

## Platform
- Colony is both a software factory and an app platform that takes whatever shape the data and workflows need.
- A data layer of collections and records sits beside files.
- A shape is a named arrangement of viewports, data, agents and commands.
- Everything is shareable in a marketplace, as packages that declare what they need.
- Shared rooms hold members, roles, data and apps. They are a separate implementation from Bellman rooms.
- The goal is an everything app.

## How it gets built
- Five stages: shell kernel, named agents, data and shapes, packages, rooms.
- The first release is a thin slice through stages 1 and 2: one named agent, end to end. The lead miner is that agent.
- See `/colony/first-slice-kickoff.md`.

## Added 2026-11-11
- The agent loop for the first slice is `colony-agent`, already in the repository. The existing heartbeat scheduler is extended, and `colony-events` grows into the daemon.
- A shape opens as an instance, bound to a project or a room. Several instances of one shape can be open at once, each as a tab in the app: for example two software factories on different projects beside a CRM. A tab is only a view, and closing it stops nothing. Not part of the first slice.

## 2026-11-11: two repos

- `colony-sh/colony` is the ADE repo. It starts with the design documents and a README, and no code.
- `colony-sh/colony-shell` stays the engine. The first slice is built there, start to finish. Nothing moves between the repos during the slice.
- The shell's command stays `colony`. The daemon is `colonyd`.
