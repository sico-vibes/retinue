# T3 Code's skill, and a Retinue skill proposal

Date: 2026-11-14. Research only. No code changed.

Evidence key: **[V]** verified by fetching the repository file or Retinue source in this session. **[U]** unverified (a third-party page, a summary of a page I could not read in full, or an inference).

## 1. What T3 Code ships (repository `pingdotgg/t3code`)

**Overall finding [V].** The repository has no skill that is installed for end users when T3 Code is set up. Its only skills are developer-facing, and its documented route for outside agents is an MCP server plus a CLI, not a skill.

- Top-level layout [V]: `https://github.com/pingdotgg/t3code` lists `.agents/skills`, `.claude`, `AGENTS.md`, `CLAUDE.md`, `docs`, `t3.json`.
- `.agents/skills/` holds three folders [V]: `contribution-triage`, `test-t3-app`, `test-t3-mobile` (`https://github.com/pingdotgg/t3code/tree/main/.agents/skills`).
- **Format [V] for `test-t3-app`.** The folder holds `SKILL.md` plus `agents/` and `references/` subfolders (`.../tree/main/.agents/skills/test-t3-app`). The frontmatter has `name: test-t3-app` and a description: "Covers testing T3 Code's web and desktop UI through the built-in Browser panel, against isolated development state." (`https://github.com/pingdotgg/t3code/blob/main/.agents/skills/test-t3-app/SKILL.md`). It is the standard `SKILL.md` frontmatter-plus-body format, the same shape Retinue already reads.
- **Sections of `test-t3-app/SKILL.md` [V, summary of the fetch].** "Test T3 web and desktop", "Start the app", "Drive agent turns for free", "Use the Browser panel", "Verify and retain". It names `references/sqlite-fixtures.md`, the sibling `test-t3-mobile` skill, the commands `vp run dev` and `node apps/server/src/bin.ts`, and browser tools such as `preview_snapshot`. It warns: never run against `~/.t3/userdata`. Replay providers avoid paid model use.
- **Not verified [U].** I could not read the body of `contribution-triage` or `test-t3-mobile`, nor the `agents/` folders. The fetch returned only file listings for them.

**What the outside-agent doc says [V].** `https://github.com/pingdotgg/t3code/blob/main/docs/user/outside-agents.md` describes connecting Claude Code and Codex over remote MCP at `https://<environment-address>/mcp`, with a sign-in that sets an access level: "Read only, Supervised, or Full access." It gives `claude mcp add --transport http t3 ...` and `codex mcp add t3 --url ...`. It also lists a `t3 thread start | send | show | list` CLI. The page states: "The page does not mention skills." OpenCode is not named. That is the fetch summary's wording, so treat it as close to the page rather than verbatim.

**Config [V].** `t3.json` only defines two launch scripts ("Dev", "Setup Worktree") with `stopOnSettle` and `runOnWorktreeCreate` flags. It has nothing about skills (`https://github.com/pingdotgg/t3code/blob/main/t3.json`).

**Third-party noise [U].** A community "t3-code" skill exists at `https://claudskills.com/skills/t3-code/`, to be copied into `~/.claude/skills/t3-code/`. It is not shown to come from pingdotgg, and I did not verify its contents. Aggregators list many T3 skills under other publishers (for example `https://www.skills.sh/itzptk/skills/t3code`). Do not treat these as official.

## 2. How skills reach agents, and per-CLI discovery

- **Repo-internal skills** are discovered by agents working in the T3 repository (`.agents/skills/`, with `AGENTS.md` and `CLAUDE.md` at the root). **[U]** I did not confirm how `CLAUDE.md` or `AGENTS.md` point to them, or whether a symlink or copy is involved.
- **Outside agents** get T3's capabilities through MCP plus the `t3` CLI, per the outside-agents doc. The first connect runs an approval page with an access level.
- **Per-CLI discovery [U, from third-party sources only].** Claude Code loads `SKILL.md` folders from `~/.claude/skills/`. Codex uses `$CODEX_HOME/skills` (default `~/.codex/skills`). Source: `https://openskillindex.com/skills/openai-codex-skill-installer` and `https://itecsonline.com/post/codex-cli-agent-skills-guide-install-usage-cross-platform-resources-2026`. OpenCode's skill path was not checked.

## 3. Keeping it up to date

**[V] for what is shown.** The repo skills are versioned with the code in the same repository, so they change with the app's commits. **[U]** No regeneration step or update mechanism for end users was found.

## 4. Conditional skills

**[V] none found.** `t3.json` has `runOnWorktreeCreate` and `stopOnSettle` on launch scripts, but no feature-gated skill. **[U]** I did not check the app source (`apps/`) for conditional skill loading.

## Retinue, for comparison (all [V] unless marked)

- **Existing developer skill:** `.claude/skills/retinue-development/SKILL.md`, which AGENTS.md requires every agent to read before work. It already has `references/` files.
- **Shared skill pool (already built):** `crates/retinue-daemon/src/skills.rs`. Skills live in `<data_dir>/skills/<name>`. They are imported from the Claude, Codex and OpenCode global folders by an explicit owner action (`SOURCES`, line 37). Every bot gets a per-turn `<retinue_skills>` index and three MCP tools: `list_skills`, `load_skill`, `read_skill_file` (lines 795–796, 869–914). The index is a prompt suffix, so it does not change the instruction hash. The header says a native copy with the same name wins.
- **Bot instruction assembly:** `crates/retinue-daemon/src/team.rs`. `runtime_instructions_in` (line 464) builds the per-bot text. `attach` (line 548) sets `options.instructions` and the per-session MCP servers. The OpenUI guide is `crates/retinue-daemon/src/openui_guide.md`, included at `team.rs:406` and served by the `ui_guide` tool. `OPENUI_INSTRUCTIONS` (line 403) tells bots to call it.
- **Per-bot MCP server:** `team.rs:560` registers the team bridge (`mcp-bridge`) with `RETINUE_DAEMON_ADDR` and a per-session token. The computer is a second server (`team.rs:570`).
- **Retinue MCP for outside agents:** `crates/retinue-daemon/src/external.rs`. The owner enables it (`mcp.external.enabled`, line 33). The tier is `chat`, `operator` or `bypass` (line 35, `ALLOW_FULL_ACCESS` at line 37). The status shows the command `claude mcp add retinue -- "<bridge>" mcp-server --endpoint "<file>"` (line 230). Tools include `list_bots`, `send_message`, `read_conversation`, `wait_for_turn`, `fan_out`, `list_projects`, `list_conversations` (`ipc-contract.md`, around lines 552–554).
- **Cloud computer:** `crates/retinue-daemon/src/computer.rs`. `workplace_on` (line 1011) reads the per-employee `computerWorkplace` switch. `workplace_instructions` (lines 1018–1034) returns a "## Your workplace" section only when the switch is on and `enabled_link` succeeds. It names the computer, never its address or token. The bridge is `vps-computer` (`SERVER_NAME`, line 49). The wake-up runs before a conversation's first turn (`before_first_turn`, line 1040). Relevant tools are `computer_exec` and `computer_create_routine`.
- **Frontend:** `apps/desktop/src/ui/ComputerPage.tsx`, not read in this session.

## Proposal

### A. User-facing "Retinue" skill for the owner's own agents (Claude Code and others)

**What it contains.** What Retinue is in two sentences. The list of MCP tools and their tiers (viewer, chat, operator), taken from `external.rs` and `ipc-contract.md`. How to find an employee (`list_bots`, then `send_message` with `botId`). Conversation IDs (`sessionId`), and that `wait_for_turn` and `read_conversation` read the reply. The rule that busy conversations queue. The rule that unknown usage is shown as unknown. Nothing else.

**Must never go in it (per AGENTS.md).** Credentials, tokens, the capability, the endpoint file path, the bridge path, host paths, environment values, message content, and provider credentials.

**Where it is installed.** `~/.claude/skills/retinue/SKILL.md` for Claude Code. Other CLIs get their own copy in their own skill folder if the owner asks.

**How the app installs and updates it, with consent.**
1. The Settings page for the external MCP (where the `claude mcp add` command already shows) gets an "Install the Retinue skill" button. The owner clicks it. This is a write outside the data folder, so it needs that explicit click.
2. The file carries a version line in frontmatter. On startup the app compares it with its bundled copy and shows a prompt to update. It never overwrites silently.
3. Turning the external MCP off offers to remove the skill. It only removes files it wrote, identified by a marker.

**Risk.** Claude Code would load a skill that describes tools the MCP may not expose. The skill must say that the tools shown by the MCP are the real list.

### B. In-app employee skill for bots inside Retinue (for example the CEO)

**What it contains.** How to orchestrate: when to delegate with `message_blob` versus `assign_task`, when to use `fan_out`, when to propose a hire, and the approval flow for consequential changes (`team.rs:485` already states these rules). It would also teach the workplace and task etiquette. This largely duplicates text that `runtime_instructions_in` already injects. The skill would be the longer, on-demand version. The CEO prompt is always sent, so the skill should carry only what is too long for every turn.

**Delivery, per provider.** Retinue already has a better channel than writing native files: the shared skill pool (`skills.rs`). Add a built-in, read-only pool entry, for example `retinue-ceo`, with a source of `builtin`. The existing index injects it for every bot, and `load_skill` and `read_skill_file` deliver it. This works the same for Claude Code, Codex and OpenCode, because it uses Retinue's own MCP tools and prompt suffix, not each CLI's skill folder.
- Optional native copy: for Claude Code, Codex (`$CODEX_HOME/skills`) and OpenCode, the daemon could also write the folder to each CLI's skill directory. This is not needed, and it would give three copies to keep in sync. Skip it unless a CLI can only find skills natively.
- Compared with the existing instruction injection: `runtime_instructions_in` is always sent and hashed (a change forces a new provider thread, per `team.rs` comments). A pool skill is loaded on demand and is not hashed. Put the stable rules in the injected text and the long playbook in the skill.

**Verification needed [U].** That the pool index is shown to every provider in practice, and that bots call `load_skill` when prompted. Test on each provider.

### C. Cloud computer sub-skill, on only while the computer is on

**What it contains.** The static how-to: where to clone and build (`/config/work/<project>`), running long jobs in the background with logs, using `computer_create_routine` for schedules, and reporting. It must not contain the computer's address, token or Harbor credential. The name of the computer is fine (`last_name`, computer.rs:457), since the existing section already uses it.

**When it is on.** Enabled exactly when `computer::workplace_on(agent)` is true and `enabled_link` succeeds (`computer.rs:1011`, `286`). That is the same gate that already produces "## Your workplace" (line 1018). So the skill's listing is generated per turn by the same check, not stored as a file.

**Removal.** Because the pool index is rebuilt each turn (`index_for_prompt`, `skills.rs:869`), removing the skill when the switch goes off needs no file deletion. The index hash changes and the skill drops out. For Claude Code native copies (if ever used) the daemon would delete the file.

**Minimal change.** The existing `workplace_instructions` already does most of this job. Moving its body into a skill would let a bot read more detail on demand, and the injected paragraph would shrink to one line pointing to `load_skill`. This is optional; the current injected text already satisfies "understands 'work on the cloud' immediately."

### Things that must never go in any skill (AGENTS.md)

Credentials, API keys, tokens and the capability; the endpoint file path and the bridge path; host paths (`C:\...`, `D:\...`); environment values; message text or transcripts; provider authentication. Keep the skills static text plus tool names.

## Open items (unverified)

1. The body of `contribution-triage` and `test-t3-mobile`, and the `agents/` folders, were not read.
2. How T3's `CLAUDE.md` or `AGENTS.md` points to `.agents/skills` was not checked.
3. OpenCode's skill discovery path was not checked.
4. The Retinue pool index's real delivery to each provider was not tested; the current analysis is from source reading.
5. Retinue's `ComputerPage.tsx` was not read, so the Settings toggle wording is unverified.
6. A threat-model note: a user-facing skill that describes the MCP tiers is information, not a secret, but the skill must not describe how to bypass tiers.

## Sources

- https://github.com/pingdotgg/t3code
- https://github.com/pingdotgg/t3code/tree/main/.agents/skills
- https://github.com/pingdotgg/t3code/tree/main/.agents/skills/test-t3-app
- https://github.com/pingdotgg/t3code/blob/main/.agents/skills/test-t3-app/SKILL.md
- https://github.com/pingdotgg/t3code/blob/main/docs/user/outside-agents.md
- https://github.com/pingdotgg/t3code/blob/main/t3.json
- https://claudskills.com/skills/t3-code/ (third-party)
- https://openskillindex.com/skills/openai-codex-skill-installer (third-party)
- https://itecsonline.com/post/codex-cli-agent-skills-guide-install-usage-cross-platform-resources-2026 (third-party)
- Retinue: `crates/retinue-daemon/src/skills.rs`, `team.rs`, `external.rs`, `computer.rs`, `main.rs` (around line 4620), `docs/ipc-contract.md`
