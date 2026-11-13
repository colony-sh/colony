# Colony

An agent development environment and app platform, built on the [Colony shell](https://github.com/colony-sh/colony-shell).

This repository is the home of the Colony ADE application. Work on the application has not started, so for now it holds the design docs. The engine, and all first-slice work, lives in `colony-sh/colony-shell`. The shell's command stays `colony`; its daemon is `colonyd`.

## The idea

- A person and agents share one shell session, each in their own lane. Bash is the language; extensions are builtins such as `claim`, `wait`, `agent`, `heartbeat` and `container`.
- Everything has a `colony://` address, and every state change is one event in a totally ordered log.
- Named agents wake on heartbeats, work inside containers, and send reports with evidence to a viewport you can answer from your phone.
- On top of that: a data layer, shapes, packages in a marketplace, and shared rooms.

Rust, plus bash. Local-first: nothing leaves the machine without a grant.

## First slice

One named agent, end to end: the lead miner. It runs once a day, drafts replies, and reports them. It sends nothing without your answer. See [`docs/first-slice/kickoff.md`](docs/first-slice/kickoff.md).

## Docs

- [`docs/decisions.md`](docs/decisions.md): decisions made so far
- [`docs/schema.md`](docs/schema.md): the types, as a design sketch
- [`docs/rules.md`](docs/rules.md): invariants and open questions
- [`docs/first-slice/kickoff.md`](docs/first-slice/kickoff.md): scope, build order and acceptance criteria
- [`docs/first-slice/gaps.md`](docs/first-slice/gaps.md): inventory of colony-shell against the slice
