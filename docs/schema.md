# Colony schema

Types, in dependency order. A design sketch, not compiled code. Generated from the schema page on 2026-11-11.

## Addresses

Everything a person or an agent might point at has a name that works from any device attached to Colony. An address is a name, not a permission.

```rust
// colony://buildbox/s3/job/21             inside a session
// colony://buildbox/agent/ci-watch/run/2  a named agent and one of its runs
// colony://local/view/reports             a viewport
// colony://local/shape/factory            a shape
// colony://local/data/tickets/142         a record
// colony://market/northwind/crm@1.2.0     a package
// colony://buildbox/room/studio           a shared room
// colony://buildbox/container/c7         a container
// colony://local/instance/colony-shell   one open instance of a shape
pub struct Address {
    pub host: HostId,
    pub scope: Scope,
}

pub enum Scope {
    Session(SessionId, Target),
    Agent(AgentName, Option<RunId>),
    View(ViewName),
    Shape(ShapeName),
    Data(CollectionName, Option<RecordId>),
    Package(PublisherId, PackageName, Option<SemVer>),  // host is market
    Room(RoomName, Option<Box<Scope>>),  // a room, or anything inside one
    Container(ContainerId),
    Instance(InstanceId),
}

pub enum Target {
    Session,
    Lane(LaneId),
    Job(JobId),
    Output(OutputHandle),
    Claim(ClaimId),
    Event(Seq),
    Worktree(WorktreeId),
}
```

## Actors and lanes

Every actor gets a lane with its own working directory, environment and history. The session underneath is shared.

```rust
pub enum ActorKind { Human, Agent, System }

pub struct Actor {
    pub id: ActorId,      // stable for the session
    pub kind: ActorKind,
    pub lane: LaneId,
}

pub struct Lane {
    pub id: LaneId,
    pub worktree: Option<WorktreeId>,  // agents get one by default
    pub sandbox: Sandbox,              // where its commands run
    pub cwd: PathBuf,
    pub env: EnvMap,
}
```

## Machines

Each machine runs a small daemon that owns its sessions, claims and event logs. Colony connects to it, over SSH for remote machines, so work survives a dropped connection.

```rust
pub struct Host {
    pub id: HostId,  // "local", "buildbox"
    pub transport: Transport,
}

pub enum Transport {
    Local,
    Ssh { destination: String },  // user@host, connects as you
}
```

## Liveness

Every lane, job and machine sends a pulse on its own channel and says what it is doing. Liveness, leases and presence in the interface derive from it. This is separate from heartbeats, which wake agents.

```rust
pub struct Pulse {
    pub source: Address,  // a lane, job or machine
    pub n: u64,           // counts up per source
    pub at: SystemTime,
    pub status: Status,
}

pub struct Status {
    pub phase: String,          // "compiling", "waiting for input"
    pub progress: Option<f32>,  // 0.0 to 1.0 when known
    pub since_last: u32,        // events produced since the last pulse
}

pub struct PulseSpec {
    pub every: Duration,  // expected interval
    pub late_after: u8,   // missed pulses before the source is late
}

pub enum Liveness {
    Alive,
    Late { missed: u8 },  // quiet, grace period running
    Dead,                 // grace ran out
}
```

## Worktrees

Built into the shell so a lane can be bound to one. An agent lane starts in its own worktree, and merging back is a command you approve.

```rust
pub struct Worktree {
    pub id: WorktreeId,
    pub repo: PathBuf,
    pub path: PathBuf,
    pub branch: String,
    pub base: CommitId,  // where it forked from
    pub owner: ActorId,
}

pub enum MergeOutcome {
    Merged { commit: CommitId },
    Conflicts { files: Vec<PathBuf>, diff: DiffHandle },
}
```

## Containers

The sandbox for programs Colony does not control: bash, compilers, any existing binary. A container gets only the files, network and credentials it was granted. Its writes land in its own layer, so committing is a transaction and discarding is the rollback.

```rust
pub struct Container {
    pub id: ContainerId,        // colony://buildbox/container/c7
    pub image: ImageRef,
    pub mounts: Vec<Mount>,     // usually one worktree
    pub net: Vec<HostPattern>,  // empty = no network
    pub limits: Limits,         // cpu, memory, wall time
    pub layer: LayerId,         // its writable layer
    pub owner: ActorId,
}

pub struct Mount {
    pub source: Resource,
    pub target: PathBuf,
    pub mode: MountMode,
}

pub enum MountMode {
    ReadOnly,
    Overlay,  // writes land in the layer, not in the source
}

pub enum LayerOutcome {
    Committed { changes: Vec<PathBuf> },  // applied through versioned writes
    Discarded,
}

pub enum Sandbox {
    Container(ContainerId),
    Isolate(IsolateId),  // code Colony hosts itself
    Native,  // the operating system's own sandbox, for what cannot run in a container
    None,    // only for commands you run yourself
}
```

## Isolates

The sandbox for code Colony hosts itself: component logic, package scripts, record triggers and heartbeat gates. An isolate can reach the outside only through the bindings it is handed, so its effects are exact and nothing is ambient.

```rust
pub struct Isolate {
    pub id: IsolateId,
    pub module: ModuleRef,       // TypeScript first, WebAssembly later
    pub bindings: Vec<Binding>,  // the only way out
    pub limits: IsolateLimits,
    pub trust: Trust,
}

pub enum Binding {
    Records { collection: CollectionName, write: bool },
    Events(EventPattern),  // read the log
    Propose,               // put a command up for approval
    Report(ViewName),      // post to a viewport
    Fetch(HostPattern),    // only if the package was granted the host
    State,                 // a small key-value store of its own
}

pub struct IsolateLimits {
    pub cpu: Duration,  // per call
    pub memory: u64,
}

pub enum Trust {
    Own,         // yours or your agent's: runs in the app's process
    ThirdParty,  // from a package: runs in a separate, sandboxed process
}

pub enum Logic {
    Declarative(ViewSpec),
    Code(ModuleRef),  // runs in an isolate
}
```

## Resources

The things two actors can collide on. Files are listed so effects can name them, but they are versioned, never claimed. Paths resolve inside the lane's worktree on the session's machine.

```rust
pub enum Resource {
    File(PathBuf),      // optimistic: see Versioned writes
    Tree(PathBuf),      // a whole subtree, e.g. node_modules/
    Stdin(JobId),       // one writer per running process
    GitIndex(PathBuf),  // repo root
    Port(u16),
    Named(String),      // "pkg:npm", "db:migrations"
}
```

## Claims

Exclusive hold on a resource that cannot be merged. Reading never needs one. The intent string is what the other actor sees. A claim lives while its holder's lane keeps its pulse.

```rust
pub struct Claim {
    pub id: ClaimId,
    pub resource: Resource,
    pub holder: ActorId,
    pub intent: String,              // "installing deps"
    pub command: Option<CommandId>,  // claim ends with this command
    pub acquired: Seq,               // the event that created it
    pub lease: Lease,
}

pub struct Lease {
    pub source: Address,  // whose pulse keeps the claim alive
    pub grace: Duration,  // agent lanes: seconds. human lanes: minutes.
}

pub enum ClaimError {
    Held {
        by: ClaimId,
        holder: ActorId,
        intent: String,
        holder_liveness: Liveness,
    },
}
```

## Versioned writes

A write names the version it was based on. If the file moved on, nothing is written and the actor gets a diff to re-plan from.

```rust
pub struct Version(pub [u8; 32]);  // content hash

pub struct Write {
    pub path: PathBuf,
    pub base: Option<Version>,  // None = file must not exist yet
    pub actor: ActorId,
    pub patch: Patch,
}

pub enum WriteOutcome {
    Committed { version: Version },
    Stale {
        current: Version,
        changed_by: ActorId,
        diff: DiffHandle,
    },
}
```

## Effects

What a command will touch. The source says how far to trust it: Observed arrives only after the run, which is too late to prevent an overlap.

```rust
pub struct Effects {
    pub reads: Vec<Resource>,
    pub writes: Vec<Resource>,
    pub deletes: Vec<Resource>,
    pub net: Vec<HostPattern>,
    pub source: EffectSource,
}

pub enum EffectSource {
    Declared,  // manifest shipped for a known tool
    Inferred,  // parsed from argv
    Observed,  // sandbox trace, known after the run
}
```

## Events

One totally ordered log per session. Every claim transition and state change is exactly one event. The claim set is a view rebuilt by replaying it. Pulses stay out of the log, and only a change in liveness is an event. Heartbeats are in it, both the firing and the answer.

```rust
pub struct Event {
    pub seq: Seq,            // total order for the session
    pub at: SystemTime,
    pub actor: ActorId,
    pub cause: Option<Seq>,  // the event that led to this one
    pub visibility: Visibility,
    pub kind: EventKind,
}

pub enum Visibility { Shared, Private(LaneId) }

pub enum EventKind {
    // commands
    CommandProposed   { id: CommandId, argv: Vec<String>, effects: Effects, via: Option<ComponentId> },
    ApprovalRequested { id: CommandId },
    ApprovalResolved  { id: CommandId, decision: Decision },
    CommandStarted    { id: CommandId, job: JobId },
    CommandPaused     { id: CommandId, reason: String },
    CommandFinished   { id: CommandId, status: ExitStatus, output: OutputHandle },
    InputRequested    { job: JobId, prompt: String, schema: InputSchema },
    InputProvided     { job: JobId },  // content is never logged

    // claims
    ClaimAcquired     (Claim),
    ClaimDenied       { resource: Resource, requester: ActorId, held_by: ClaimId },
    ClaimReleased     { id: ClaimId },
    ClaimExpired      { id: ClaimId },
    ClaimPreempted    { id: ClaimId, by: ActorId, reason: String, command: Disposition },
    ClaimTransferred  { id: ClaimId, from: ActorId, to: ActorId },

    // state
    FileChanged       { path: PathBuf, from: Option<Version>, to: Option<Version>, diff: DiffHandle },
    WriteRejected     { path: PathBuf, base: Option<Version>, current: Version },
    StatePromoted     { key: StateKey, value: String },  // lane to shared
    Handback          { job: JobId, summary: String },

    // heartbeats
    HeartbeatSet      { heartbeat: Heartbeat },
    HeartbeatFired    { id: HeartbeatId, beat: u64, run: RunId },
    HeartbeatAnswered { id: HeartbeatId, beat: u64, outcome: BeatOutcome },
    HeartbeatPaused   { id: HeartbeatId, by: ActorId },

    // agents
    AgentCreated      { agent: Agent },
    AgentStateChanged { name: AgentName, from: AgentState, to: AgentState },
    ReportPosted      { agent: AgentName, run: RunId, status: ReportStatus, view: ViewName },
    AskAnswered       { agent: AgentName, run: RunId, ask: AskId, by: ActorId },

    // data, shapes, packages
    RecordChanged     { collection: CollectionName, id: RecordId, from: Option<Version>, to: Option<Version> },
    MigrationApplied  { collection: CollectionName, to: u32 },
    ShapeChanged      { name: ShapeName, from: Version, to: Version },
    ShapeSwitched     { to: ShapeName },
    InstanceOpened    { instance: Instance },
    InstanceClosed    { id: InstanceId },
    PackageTrialed    { package: PackageRef, touched: Effects },
    PackageInstalled  { package: PackageRef, granted: Needs },
    UpdateOffered     { package: PackageRef, from: SemVer, grew: Needs },
    PackagePublished  { package: PackageRef, lineage: Option<PackageRef> },

    // rooms
    RoomCreated       { room: Room },
    MemberJoined      { room: RoomName, who: Identity, role: RoleName },
    MemberLeft        { room: RoomName, who: Identity },
    RoleChanged       { room: RoomName, who: Identity, from: RoleName, to: RoleName },
    HandoverRequested { claim: ClaimId, by: ActorId },

    // containers
    ContainerStarted  { container: Container },
    ContainerStopped  { id: ContainerId, status: ExitStatus },
    LayerCommitted    { id: ContainerId, changes: u32 },
    LayerDiscarded    { id: ContainerId },
    IsolateFailed     { id: IsolateId, module: ModuleRef, reason: String },

    // worktrees
    WorktreeCreated   { worktree: Worktree, lane: LaneId },
    WorktreeMerged    { id: WorktreeId, into: String, outcome: MergeOutcome },
    WorktreeRemoved   { id: WorktreeId },

    // presence
    LaneOpened        { lane: LaneId },
    LaneClosed        { lane: LaneId },
    LivenessChanged   { source: Address, from: Liveness, to: Liveness },

    // interface
    ComponentInstalled { component: Component },
}

pub enum Decision { Once, Pattern(String), Deny { reason: String } }
pub enum Disposition { Paused, Cancelled }
pub enum StateKey { Cwd, Env(String) }
```

## Waits

What an actor can block on instead of polling. A queued command is a Wait::Released on the resource it was denied.

```rust
pub enum Wait {
    Released(Resource),
    Job(JobId, JobCond),
    Event(EventPattern),
    Liveness(Address, Liveness),  // a source turns late or dead
    Any(Vec<Wait>),
    Deadline(Box<Wait>, Duration),
}

pub enum JobCond { Exited, PortOpen(u16), LogMatch(Regex) }
```

## Heartbeats

A standing wake-up for an agent. The daemon fires it, the agent looks at the brief and must answer. It carries its own grants and budget, because nobody is watching when it runs.

```rust
pub struct Heartbeat {
    pub id: HeartbeatId,
    pub agent: AgentName,          // the named agent to wake
    pub trigger: Trigger,
    pub brief: String,             // what to look at on each beat
    pub gate: Option<Gate>,        // cheap check; wake the model only if it passes
    pub grants: Vec<Capability>,   // what a beat may do unattended
    pub budget: Budget,
    pub overlap: Overlap,          // the previous beat is still running
    pub backoff: Option<Backoff>,  // slow down after idle beats
    pub paused: bool,
}

pub enum Trigger {
    Every(Duration),
    Cron(String),
    OnEvent { pattern: EventPattern, debounce: Duration },
    OnRecord { collection: CollectionName, filter: Option<Query> },
    OnQuiet(Duration),  // nothing has happened in the session for this long
}

pub struct Budget {
    pub wall: Duration,
    pub tokens: u64,
    pub commands: u32,
}

pub enum Overlap { Skip, Queue, Coalesce }

pub enum Gate {
    Command(Vec<String>),
    Code(ModuleRef),  // runs in an isolate
}

pub struct Backoff {
    pub after_idle: u8,  // idle answers in a row
    pub factor: f32,
    pub max: Duration,
}

pub enum BeatOutcome {
    Idle,                             // looked, nothing to do
    Acted { summary: String },
    NeedsHuman { question: String },  // parks until someone answers
    Missed,                           // no answer inside the budget
}
```

## Named agents

An agent with a name that outlives any session. It persists as a charter, notes and a log, not as a long-running process. Every run starts with a fresh context, loads those, and is its own session.

```rust
pub struct Agent {
    pub name: AgentName,        // "ci-watch", unique per machine
    pub home: HostId,           // the machine whose daemon runs it
    pub charter: String,        // its standing task
    pub grants: Vec<Capability>,
    pub daily: Budget,          // across all runs in a day
    pub heartbeats: Vec<HeartbeatId>,
    pub notes: Vec<Note>,       // what it carries between runs
    pub worktrees: WorktreePolicy,
    pub image: Option<ImageRef>,  // each run gets a fresh container from it
    pub reports_to: Address,    // a viewport
    pub state: AgentState,
}

pub enum AgentState {
    Sleeping,
    Running(RunId),
    Parked(RunId),  // waiting on an answer from you
    Paused,
    Retired,
}

pub enum WorktreePolicy { FreshPerRun, Reuse(WorktreeId), None }

pub struct Note {
    pub text: String,
    pub about: Option<Address>,    // what it describes
    pub seen_at: Option<Version>,  // re-check before acting if this has moved
}
```

## Reports

What an agent sends you. Evidence is a list of addresses, so every claim links to the job output or diff behind it. An ask parks the run until someone answers.

```rust
pub struct Report {
    pub agent: AgentName,
    pub run: RunId,
    pub at: SystemTime,
    pub status: ReportStatus,
    pub headline: String,
    pub body: String,
    pub evidence: Vec<Address>,  // jobs, outputs, diffs
    pub asks: Vec<Ask>,
}

pub enum ReportStatus { Fine, Attention, Blocked, NeedsYou }

pub struct Ask {
    pub id: AskId,
    pub question: String,
    pub options: Vec<String>,         // empty = free text
    pub approves: Option<CommandId>,  // a yes resolves this approval
    pub to: Option<RoleName>,         // in a room, the role that may answer
}
```

## Viewports

An addressable surface in Colony. Opening its address on your phone or your desktop shows the same state, including what you have read. Agents post reports to it and cannot change its layout.

```rust
pub struct Viewport {
    pub name: ViewName,          // colony://local/view/reports
    pub component: ComponentId,  // how it renders
    pub sources: Vec<AgentName>, // whose reports land here
    pub read_upto: Seq,          // shared across your devices
}
```

## Data

Records beside files, for the app side of the platform. A record write carries its base version like a file write. A schema change is a migration that you approve.

```rust
pub struct Collection {
    pub name: CollectionName,  // "tickets"
    pub schema: Schema,
    pub schema_version: u32,
    pub triggers: Vec<ModuleRef>,  // run in isolates when a record changes
}

pub struct Schema { pub fields: Vec<Field> }

pub struct Field {
    pub name: String,
    pub kind: FieldKind,
    pub required: bool,
}

pub enum FieldKind {
    Text,
    Number,
    Bool,
    Time,
    Choice(Vec<String>),  // a workflow's states live here
    Ref(CollectionName),  // link to another record
    Address,              // link to a job, diff, run or report
}

pub struct Record {
    pub id: RecordId,
    pub version: Version,  // same optimistic rule as files
    pub fields: Map<String, Value>,
}

pub struct Migration {
    pub collection: CollectionName,
    pub from: u32,
    pub to: u32,
    pub steps: Vec<MigrationStep>,  // proposed like a command, approved like a merge
}
```

## UI components

A panel an agent adds to Colony. It renders from the event log and from the addresses it was granted. It holds no claims, and a command it issues is proposed in the name of whoever clicked.

```rust
pub struct Component {
    pub id: ComponentId,
    pub author: ActorId,             // usually the agent that built it
    pub subscribes: Vec<EventPattern>,
    pub reads: Vec<AddressPattern>,  // what it may display
    pub proposes: bool,              // may it put commands up for approval
    pub view: Logic,                 // declarative when it can be, isolate code when it needs logic
}

pub struct ViewSpec { pub widgets: Vec<Widget> }

pub struct Widget {
    pub kind: WidgetKind,
    pub source: Query,  // over a collection or the event log
}

pub enum WidgetKind { Table, Form, Detail, Board, Chart, Timeline, Lane, Diff }
```

## Shapes

A named arrangement of Colony: which viewports, data, agents and commands are on screen. Switching shape rearranges the interface. The shell, log, agents and grants stay the same underneath. The software factory is one shape.

```rust
pub struct Shape {
    pub name: ShapeName,  // colony://local/shape/factory
    pub layout: Layout,   // which viewports go where
    pub viewports: Vec<ViewName>,
    pub collections: Vec<CollectionName>,
    pub agents: Vec<AgentName>,
    pub commands: Vec<String>,     // what the command bar offers
    pub version: Version,          // every change is undoable
    pub from: Option<PackageRef>,  // the package it was installed from
}
```

## Instances and tabs

A shape is a definition. What you open is an instance of it, bound to a project or a room. Several instances of one shape can be open at once, and each is a tab in the app.

```rust
pub struct Instance {
    pub id: InstanceId,   // colony://local/instance/colony-shell
    pub shape: ShapeName, // "factory", "crm"
    pub title: String,    // shown on the tab
    pub scope: InstanceScope,
    pub layout: Layout,   // restored when the tab is reopened
}

pub enum InstanceScope {
    Project { machine: HostId, repo: PathBuf },  // a factory bound to one repository
    Room(RoomName),                              // a shared room and its data
    Private(SessionId),                          // your own session
}

pub struct TabStrip {  // per window, not shared
    pub pinned: Vec<ViewName>,  // app-level views such as reports
    pub tabs: Vec<InstanceId>,  // in the order shown
    pub active: InstanceId,
}

pub struct TabBadge {
    pub instance: InstanceId,
    pub asks_waiting: u32,  // so a background tab can still get your attention
}
```

## Packages

The unit of sharing in the marketplace. It declares everything it needs, and the sandbox enforces that list. It never carries your records, an agent's live notes or secrets. Each install runs in the installer's own Colony on their own data.

```rust
pub struct Package {
    pub publisher: PublisherId,
    pub name: PackageName,
    pub version: SemVer,
    pub signature: Signature,
    pub lineage: Option<PackageRef>,  // the package this was forked from
    pub requires: Vec<PackageRef>,
    pub needs: Needs,
    pub contents: Vec<Item>,
}

pub struct Needs {  // shown before install, enforced after
    pub collections: Vec<CollectionGrant>,
    pub commands: Vec<String>,
    pub net: Vec<HostPattern>,
    pub machines: Vec<HostPattern>,
    pub optional: Vec<NeedId>,  // the package still works without these
}

pub enum Item {
    Shape(Shape),
    Viewport(Viewport),
    Component(Component),
    Agent(AgentTemplate),  // charter, heartbeats, note templates
    Schema(Collection, Vec<Migration>),
    Script(String),
    Module(ModuleRef),  // isolate code for components, triggers and gates
    EffectManifest(String),  // declared effects for a tool
}

pub enum InstallOutcome {
    Trial { touched: Effects, snapshot: SnapshotId },  // ran on a snapshot, kept nothing
    Installed { granted: Needs },
    Refused { reason: String },
}
```

## Rooms

A shared workspace: people and agents, a role for each, and the data, apps and agents they share. A room has one home machine with one log, and members attach to it. A session is a private room of one.

```rust
pub struct Room {
    pub name: RoomName,    // colony://buildbox/room/studio
    pub home: HostId,      // the one machine that holds its log and data
    pub members: Vec<Member>,
    pub roles: Vec<Role>,
    pub shape: ShapeName,  // the apps members see
    pub collections: Vec<CollectionName>,
    pub agents: Vec<AgentName>,
    pub budget: Budget,    // what its agents may spend
}

pub struct Member {
    pub who: Identity,
    pub role: RoleName,
    pub joined: SystemTime,
}

pub enum Identity {
    Person(PublicKey),  // the same proof on every machine
    Agent(AgentName),
}

pub struct Role {
    pub name: RoleName,  // "owner", "editor", "viewer"
    pub grants: Vec<Capability>,
    pub answers: Vec<AskKind>,  // which asks this role may answer
    pub may_preempt: bool,      // owners only, by default
}

pub struct Invite {
    pub code: String,
    pub role: RoleName,
    pub expires: Option<SystemTime>,
    pub uses_left: Option<u32>,
}
```

## From bash

The same operations as builtins, so every script stays valid bash and an agent can use them without learning new syntax.

```bash
# hold a claim for the life of one command
claim pkg:npm --intent "installing deps" -- npm install

# block until something is free, or a job reaches a state
wait --released git-index
wait --job 14 --port-open 3000 --timeout 30s

# pass a running process to the other actor, and back
handoff --job 12 --to human
handoff --job 12 --to agent --summary "signed in"

# copy lane state into the shared session
promote env DATABASE_URL
promote cwd

# worktrees
worktree new fix-auth --from main
worktree merge fix-auth --into main

# other machines, and pointing at anything on them
host add buildbox dev@buildbox
session open --host buildbox
attach colony://buildbox/s3/job/21

# liveness: report your status, or react when a lane dies
pulse --phase "compiling" --progress 0.4
wait --dead colony://buildbox/s3/lane/agent

# named agents that work on their own
agent new ci-watch --on buildbox --every 10m --charter "keep CI green on open branches"
agent list
agent pause ci-watch
agent retire ci-watch

# more heartbeats for an agent: a schedule, or when something happens
heartbeat add ci-watch --on-event CommandFinished --debounce 30s
heartbeat list ci-watch

# viewports
view open colony://local/view/reports

# shapes
shape list
shape switch factory
shape fork crm --as my-crm

# several instances of a shape, each in its own tab
shape open factory --project ~/src/colony-shell
shape open factory --project ~/src/bellman
shape open crm --room studio
tab list
tab close bellman

# data
data new tickets --field title:text --field 'state:choice(todo,doing,review,done)'
data query tickets --where state=review

# marketplace
market search crm
market try northwind/crm@1.2.0      # runs on a snapshot, keeps nothing
market install northwind/crm@1.2.0
market publish my-crm --as 0.1.0

# rooms
room new studio --on buildbox --shape crm
room invite studio --role editor --expires 7d
room join <code>
room members studio
room leave studio

# containers
container run --image rust --mount . -- cargo test
container diff c7
container commit c7
container discard c7
lane enter c7
```

## On a tuple space

How the above maps onto Linda operations. Acquire needs insert-if-absent, which classic Linda does not have. Take-a-token only works for resources known up front.

```rust
// acquire: write the claim unless one matches already
out_unless(("claim", R, _, _), ("claim", R, me, lease))

// release: take your own claim out
in(("claim", R, me, _))

// presence: read without removing
rd(("claim", _, agent, _))

// events: written once, never taken
out(("event", seq, kind, fields))

// subscribe: block on the next matching seq
rd(("event", > last_seen, pattern))
```
