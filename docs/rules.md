# Colony rules

The invariants the design assumes. Generated from the schema page on 2026-11-11.

1. One claim per resource. A Tree claim conflicts with any claim at or under its root.
2. Reading never needs a claim. Both actors can watch any file or process at any time.
3. Files are never claimed for editing. A write carries its base version and fails when that version is stale.
4. An agent lane starts in its own worktree. Merging back is a proposed command that you approve. The versioned-write rule covers any tree two actors share.
5. Every lane, job and machine sends a pulse on its own channel and reports a status. Pulses stay out of the log. Only a change in liveness is an event.
6. A claim lives while its holder's lane keeps its pulse and its command is still running. Agent lanes get a grace of seconds, human lanes minutes.
7. A heartbeat wakes a named agent with a brief. Every firing and every answer is an event, so anything an agent does unprompted traces back to the beat that woke it.
8. A beat runs with the heartbeat's own grants and budget, which are narrower than an attended session. Anything outside them is answered NeedsHuman and parks until you reply.
9. An agent answers every beat, even with Idle. A missed answer counts against its liveness, because only the model can produce one.
10. The daemon fires heartbeats, so they run on a machine whether or not Colony is attached.
11. A human preempts an agent. An agent never preempts a human. Between two agents the first claim wins and the other waits.
12. Preemption always carries a reason and says whether the command was paused or cancelled. The agent re-plans from that. A blind retry is denied.
13. Every claim transition is exactly one event. The log is the truth, and the claim set is a view that replay can rebuild.
14. Private events are stored but never delivered to other actors. A file change in a private lane is still delivered, with path and versions only.
15. Each machine runs a daemon that owns its sessions, claims and logs. Colony is a client of it. A claim never spans machines.
16. A lane that misses pulses is late, then dead once its grace runs out. Its claims survive while it is late. A dropped connection is just a late lane.
17. Sessions, lanes, jobs, outputs, claims, events and worktrees all have addresses. An address works from any device and is a name, not a permission.
18. A UI component renders from the events and addresses it was granted and holds no claims. A command it issues is proposed in the name of whoever clicked.
19. A named agent persists as a charter, notes and a log. Every run starts with a fresh context and is its own session.
20. Every run that acted ends in a report, and every claim in a report links to its evidence by address. Idle runs stay silent or roll up into a digest.
21. A note records the version of what it describes. An agent re-checks a note whose subject has moved before acting on it.
22. A viewport has an address and one read state shared across your devices. Agents post to it and cannot change it.
23. A shape is declarative data. Switching shape changes what is on screen and nothing underneath. Every change to a shape is versioned and undoable.
24. A record write carries its base version, like a file write. A schema change is a migration you approve.
25. A package declares everything it needs and the sandbox enforces it. It never carries records, live notes or secrets.
26. A package can be tried on a snapshot before install. Versions are signed and pinned, and an update whose needs grew waits for you.
27. Each install runs in the installer's own Colony on their own data.
28. A room has one home and one log. Members attach to it, and nothing is merged between copies.
29. In a room, grants attach to roles. A room's agent acts with the room's grants, never a member's own authority.
30. An ask in a room goes to a role, and the log records who answered.
31. Between two people there is no preemption, only a request to hand over, unless the role allows it. Owners may preempt by default.
32. Private lanes and personal secrets stay personal. When a member leaves, the data stays with the room.
33. A container gets only the files, network and credentials it was granted. It has its own address and pulse.
34. Writes inside a container land in its layer. Committing applies them through versioned writes. Discarding is the rollback.
35. A command with unknown effects runs in a container by default, and each run of a named agent gets a fresh one.
36. Containers run programs Colony does not control. Isolates run code Colony hosts: component logic, package scripts, triggers and gates.
37. An isolate reaches the outside only through its bindings. A package's granted needs become its bindings, so nothing is ambient.
38. What an isolate changes shows up as ordinary events, such as a record change or a proposed command, attributed to whoever it acts for.
39. Third-party isolate code runs in a separate, sandboxed process.
40. A shape is a definition. An instance is a shape bound to a project or a room, and several instances of one shape can be open at once.
41. Each instance is a tab. A tab is only a view: closing it stops nothing, and its agents keep running.
42. Nothing crosses between instances unless it is addressed. Each has its own lanes, worktrees and layout.
43. A tab shows how many asks in it are waiting for you, and the reports viewport collects them across every instance.

# Open questions

Where the design is weakest.

## What cannot run in a container

Containers settle two earlier questions: commands with unknown effects, and tools that write files directly. On a Mac they run inside a virtual machine, so mounted worktrees are slower, and Mac or iOS builds cannot run in them at all. Those commands fall back to the system's own sandbox, which can restrict access and cannot give a layer to commit or discard.

## How a code component draws

Components stay declarative when they can, and logic runs in an isolate with bindings only. What is still open is the drawing. An isolate that returns data for a fixed widget set is safe. One that produces its own markup is the largest new attack surface in the design.

## Which isolate runtime

The plan is to embed V8 from Rust. Deno's core and Cloudflare's workerd are the candidates, and neither has been evaluated. Whether the same module can draw in a phone viewport depends on the choice.

## Charters as an attack surface

A packaged agent's charter is text a model will follow, so a marketplace invites hidden instructions. Charters are shown before install, and a model should treat record contents and package text as data. The grants are the real backstop: a charter cannot do what its package was not granted.

## Who decides a package is safe

A signature proves who published, not that the package is safe. Declarative-only components keep third-party code out at first. Code components need a review process before they can be listed.

## Declined needs

Declining one need can leave a package half working. The sketch lets a package mark needs as optional. Whether a required need can be declined at all is undecided.

## Reaching a room's home

Members have to reach the one machine that holds the room. A home behind a home network needs a relay or a hosted machine. This is the part a separate implementation has to build that a hosted service would already have.

## Who a person is across machines

The sketch gives each person a key. Enrolling a second device, and recovering after a lost key, are both undecided.

## Who pays for a room's agents

A room has a budget, and the sketch does not say who funds it. Charging the owner is simplest. Splitting by who answered or who asked gets complicated quickly.

## Tabs across devices

An instance is shared across your devices. The sketch keeps the set of open tabs per window. Whether your phone should open with the same tabs as your desktop is undecided.

## Report noise

Several agents reporting every few minutes will be ignored within a week. A likely default is a daily digest per agent, with an interruption only for NeedsYou and Blocked.

## How much an agent carries between runs

Notes grow. Without pruning, each run loads more context and trusts older claims. The notes need a cap and a rule for what is dropped first.

## What a beat may do with nobody watching

Grants per heartbeat are the safety boundary for unprompted work. Too narrow and every beat parks on a question. Too wide and an agent acts on stale assumptions at 3am. A sensible start is read access plus writes inside its own worktree, with merging always a question.

## The cost of idle beats

Every beat is a model turn. The gate command avoids waking the model when a cheap check says nothing changed, backoff widens the interval after idle beats, and event triggers replace polling where an event exists. Whether that is enough needs measuring.

## A lane with a pulse and a stuck model

A pulse says the harness is alive. Only an answered heartbeat says the model is. A lane with no heartbeat set has no such proof, so long attended work may need an implicit one.

## Reject or merge on a stale write

The sketch rejects and returns a diff. A three-way merge that commits when hunks do not overlap would cut round trips, at the cost of the actor not seeing what it merged with.

## More than one agent

First claim wins is fine for two. With several agents, two can each hold what the other wants. The shell needs to find cycles in the wait graph and cancel the youngest wait.

## No shared order across machines

A sequence number orders one session on one machine. Across machines there are only cause links by address. Work that needs real coordination between machines has to run through a single session.

## Insert-if-absent in the tuple space

Acquire needs it. If the space cannot provide it atomically, claims have to go through a single arbiter task that owns the claim tuples, and the space stops being the only coordination mechanism.

## How long you wait on an agent

When you are denied, the default is to ask. A per-resource default would save prompts: always wait for package installs, always preempt for stdin.
