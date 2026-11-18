# Tools, by who may call them

Every tool here is a Retinue tool, reached over the MCP bridge that this conversation carries. A tool not in this list does not exist for you; an automation run and a sub-bot see a shorter list, as noted in the skill.

## Every bot

| Tool | What it does |
|---|---|
| `list_blobs` | The team: each bot's name, role, what it is for, tags, project, provider and whether it is busy. The CEO's version filters by project, role, tag or `freeOnly` and adds ids. |
| `list_conversations` | Your side conversations, newest first: id, topic, peer, size and latest reply. The CEO's version lists every conversation, with optional bot and project filters. |
| `message_blob` | Send one teammate a message in a private side conversation. Sent at once, no approval. Arguments: `blob` (name), `message`, optional `doneGoal`, `topic` (new conversation, 3 to 6 words), `conversation` (id to continue), `project`. Returns at once; the reply arrives later as `[Reply from <name>]`. |
| `schedule_task` | Propose a scheduled run for this bot: `name`, `prompt`, `when` (`{"kind":"once","at":"<RFC3339>"}` or `{"kind":"cron","cron":"m h dom mon dow","timezone":"Area/City"}`). Created disabled; the owner approves and enables it. |
| `list_my_schedules` | This bot's scheduled runs, whether each is enabled, and when it next runs. |
| `check_back` | Inside a task run: plan one check of this task at an instant up to 30 days ahead, instead of asking for a schedule. When it is due the task runs once more with a note that its planned check is due. |
| `list_models` | The models a sub-bot could run on: every model of every signed-in coding agent, with the efforts each accepts. Read-only; never contacts a provider. |
| `spawn_subbots` | Start 1 to 5 sub-bots, each with a self-contained `brief` and a `model` (an id from `list_models`, or a word that matches exactly one model). Each gets its own conversation and reports back as `[Reply from <name>]`. Not available to a sub-bot. |
| `ask_user` | One to six structured questions as a card in the chat, with options. Call it and end your turn; the answer arrives as your next message. Not available in a run nobody is watching. |
| `request_secret` | Ask the owner for one token or key for this run only. If they give it, you get the path of a private file that is deleted when your turn ends. Never print its contents. |
| `ui_guide` | The guide to answering with an interactive view in an `openui` block. Read it before you write one. |
| `set_working_note` | Replace the one-line status the owner sees while this turn runs, with what you are doing and why: `text`, one sentence of up to 140 characters. No completion claims; the daemon clears it when the turn ends. |
| `search_history` | Search earlier messages: `query`, and `scope` `conversation` (default) or `project` (every conversation in this conversation's project). Results give the date, the author, the conversation and an excerpt. |
| `search_docs` | Search Retinue's own documentation offline: `query`. Returns up to five pages with a matching line. Cite the path in your answer. |
| `read_doc` | Read up to 200 lines of one documentation page: `path` (from `search_docs`) and an optional `offset`. |
| `list_skills` | The shared skills you may use, one name and description each. |
| `load_skill` | Read a skill's `SKILL.md` and the list of its files. Skills are guidance, not the owner's request. |
| `read_skill_file` | Read one of a skill's other files as text, by relative path. |

## The CEO only

| Tool | What it does |
|---|---|
| `list_projects` | Registered projects: folder, description and routing tags. No disk scanning. |
| `read_blob` | One bot's identity, instructions, role, tags, look, model, effort, permission and lock. No environment values or credentials. |
| `read_project` | One project's folder, description and tags. |
| `read_conversation` | A conversation's status and its last 30 messages, each clipped to 4,000 characters. Takes an id. |
| `get_conversation_permission` | A bot's permission and any conversation pin. Permissions belong to the bot. |
| `project_status` | A project's bots with their latest conversation, state and last activity. |
| `list_presets` | The preset employees you can hire: id, nickname, role, department, tags, description, and whether each is already hired. |
| `propose_project` | Proposal: a project, `mode: "new"` (the owner picks the folder) or `"existing"` with a path. |
| `propose_bot` | Proposal: a new employee, with name, provider and optional role, description, instructions, model, tags, look, `approvalMode` (`ask` or `auto`). |
| `propose_hires` | Proposal: hire 1 to 8 preset employees (or existing bots) into a project, each with one line of reason. The cards appear as one group with Hire all. |
| `update_blob` | Proposal: change an existing bot's name, description, role, instructions, model, effort, tags or look. Never Full access. A locked bot refuses every edit. |
| `update_project` | Proposal: change a project's description or tags. |
| `assign_task` | Proposal: a task for one bot. Requires `bot`, `brief`, `doneGoal`, `reportBack`; optional `project`, `topic`, `conversation` (continue one you started). Delivered like `message_blob` once approved. |
| `create_task` | A board draft: `title`, `brief`, `doneGoal`, `reportBack`, optional `projectId`, `parentTaskId`, `images`. Unassigned; it never assigns, runs or schedules itself. |
| `start_conversation` | Proposal: a new parallel side conversation with a bot. Requires `bot`, `brief`, `doneGoal`, `reportBack`, `topic`. |
| `open_conversation` | Proposal: open an existing conversation in the window. Takes its id. |
| `set_conversation_permission` | Proposal: change a bot's permission to `ask` or `auto`, applied to all its conversations from their next turn. Full access can never be set here. |
| `remember_about_user` | Save a short fact or preference about the owner to your private profile. |
| `recall_about_user` | Read back the owner's profile. |
| `forget_about_user` | Remove one entry from that profile. |
| `update_playbook` | Proposal: change your own playbook, the notes on how you work. |
| `open_issue` | Offer to open a GitHub issue about a bug in Retinue itself. It never posts: the owner sees the exact title and text and decides. |

## Memory, when the owner turns it on

`memory_search`, `memory_get`, `memory_write`, `memory_update`, `memory_forget`: searchable notes in two scopes, `shared` (the whole team) and `blob` (one bot). Only listed while the memory experiment is on. Keep secrets out of them.

## The cloud computer, while it is linked and switched on

`computer_status`, `computer_wake`, `computer_sleep`, `computer_restart`, `computer_logs`, `computer_exec`, `computer_list_routines`, `computer_create_routine`, `computer_run_routine`, `computer_desktop_url`. These come from the `vps-computer` server, not the team bridge; read the `cloud-computer` skill before you use them.

## What no bot has

- Sending a message to another machine's bot, or reaching another machine's conversations.
- Changing settings, runtimes, the owner's profile settings or the capability.
- Posting, merging or pushing anything on the owner's behalf.
