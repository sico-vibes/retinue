# Working: delegation, projects, tasks, schedules

The short version lives in `SKILL.md`. This file is the detail behind it.

## Delegation

`message_blob` is ordinary delegation: one teammate, a private side conversation, delivered at once with no approval card. Use it whenever the owner asks you to message, ask or hand work to someone.

Write the brief yourself, with these lines (leave one out only when it truly does not apply):

- **Task**: the concrete job, the current and desired behaviour, and where it lives.
- **Do not touch**: what must stay as it is.
- **Context**: facts from this conversation the teammate cannot see.
- **Rules**: the project's own rules that matter, and the exact commands to run.
- **Report**: what to send back: what changed, files touched, command outcomes, anything left open.

One task per brief; split unrelated work into separate messages. A teammate never commits or pushes: the owner reviews and lands the work, so ask for the work to be left uncommitted.

After sending, tell the owner in one short sentence who you asked and end your turn. Do not wait, poll or sleep. The reply arrives later as `[Reply from <name>]`; when it arrives, report what they did or found, your own verdict, and any open questions.

**Continuing**: pass `conversation` (an id from `list_conversations`) to carry on the same topic. For rework send only the delta: what is wrong, what to keep, and the exact commands. For parallel work, start a new conversation with a new `topic` of 3 to 6 words. You can continue only a side conversation you started; if the teammate started it with you, answer there, or start your own for a new question.

**Busy**: a delivery starts a turn unless the peer is mid-turn or every concurrency slot is taken. Then it queues and starts when it can. The queue reason is recorded with the delivery, so the owner can see why it has not started.

## Projects and routing

- A project is a name, description, tags and a folder. `#Name` in the owner's message is a project, matched by name (case-insensitive, exactly one) or by a tag only one project carries. Two matches is an error that names the candidates: ask the owner with real options instead of guessing.
- Pass `project` (id, exact name, or unique tag) to `message_blob` and `assign_task`. Without it the work opens in the project of the conversation you are in. If the work belongs elsewhere and you cannot name the project, tell the owner to start a conversation there.
- Research with no project goes to a free bot tagged researcher; for project P, use P's own researcher. If none fits, the CEO proposes one for that project.

## Worktrees

A delegated conversation normally works in its project's folder. When another conversation is already busy in the same git repository, Retinue gives the new conversation its own git worktree and branch, so the two cannot clash; the teammate is told it is working in a separate copy and that the changes are offered for merge when the task is done. Bots tagged or roled as researchers are never given one. If isolation was wanted and could not be made, the conversation says why instead of claiming the folder is fine.

## Tasks

A task is a board record: title, brief, done goal, report-back, assignee, project, state. It is not a conversation, but running a task starts one.

- `create_task` (CEO) makes an unassigned draft. It never assigns, dispatches or schedules itself; the owner drags it to an employee.
- `assign_task` (CEO) proposes a tracked task for one bot. It is delivered like `message_blob` once the owner approves, and the reply comes back to the CEO's conversation.
- A task's provider turn ending proves the turn ended, never that the done goal was met. That moves a task to awaiting review; only the owner closes it.
- The owner assigns work to the board, and dispatch, retries and review are the owner's actions.

## Schedules

`schedule_task` creates a disabled automation for this bot; nothing runs until the owner approves and enables it. A schedule is a normal conversation with `link.kind = automation`, run by the scheduler.

A scheduled run is unattended and fenced: approval mode is clamped to plan or auto, unclassified permissions are auto-denied, questions are dismissed, missed occurrences coalesce into one, overlapping runs are skipped, and four consecutive failures pause it visibly. A run that failed after changing something is never retried automatically; the owner retries from the run history. If there is nothing worth the owner's attention, reply exactly `(pass)`.

`list_my_schedules` shows what this bot has and when each next runs. A run cannot change automations.

## Sub-bots

`spawn_subbots` starts 1 to 5 helpers at once. Each is its own bot row, hidden from the team list, with its own conversation, its own model and effort (a different provider is allowed), and a self-contained brief. Every entry is checked before any starts. The results arrive later as `[Reply from <name>]`.

A sub-bot cannot start sub-bots and works with no more access than the conversation that started it. Use them for independent pieces of one job, not for work that needs the owner's approval: they hit the same approval path as everyone else.

## Fan-out

Fan-out sends one prompt to 2 to 6 bots, each in its own new conversation, so the answers can be read side by side and one can be picked. It belongs to the owner and to outside agents on the Retinue MCP (`fan_out`); it is not a tool inside a bot's conversation. When the owner asks for several opinions, propose the work with `start_conversation`/`assign_task` instead, and say that fan-out is the owner's own comparison view.

## Approvals and permissions

- Approval modes are `ask` (each tool call needs a decision), `auto` (classified reads and writes run, the rest asks), `accept_edits` (edits run, the rest asks) and `bypass` (Full access, no prompts). The owner picks; a bot may propose only `ask` or `auto`.
- A conversation can carry a stricter pin, applied from the next turn. Permissions belong to the bot, not the conversation.
- A consequential change becomes a card: project or bot creation and edits, hires, task assignment, opening a conversation, permission changes. A card waits; never claim the outcome before the result arrives.
- A locked bot refuses every settings change. An archived conversation cannot message.
- Automations never carry bypass.

## The CEO's loop

Ground yourself with `list_projects` and `list_blobs`, then route by role, description, tags and project. Read the owner's message for the project first; if the work needs a project and they named none, `ask_user` with the projects as options.

When a project starts, interview briefly: the goal, the stack, what done looks like, what matters most. Then `list_presets` and `propose_hires` with the smallest team that fits, each with a one-line reason. Reuse an employee already hired instead of making a second one. `propose_bot` only for a need no preset covers, and say so.

Before reporting a finished assignment, compare the reply with its done goal. Treat a changed, skipped or deleted test as a failure until the owner agrees. Report the teammate's command outcomes as their claims, not your evidence; say which command the owner should re-run when you cannot run the gates yourself. Surface decisions the brief did not ask for rather than absorbing them silently.

When nothing changed and nothing needs the owner, say nothing or one short line.
