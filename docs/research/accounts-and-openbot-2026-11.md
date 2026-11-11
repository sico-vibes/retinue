# Accounts, sign-in, and how OpenBot runs coding bots

Research for the owner, 11 November 2026. Research only: no code was changed, no commit was made, and the clones used for reading were removed at the end (see Sources).

**What this is for.** The owner wants a person to be able to add an account and sign in to a coding agent from inside Retinue, the way OpenBot does, and separately wants to know how OpenBot runs its coding bots, and what supporting Google's Antigravity CLI would take.

**Claim markers.** Every factual statement carries one of:

- **[code]** read directly in the named repository at the commit in Sources, with `file:line`.
- **[docs]** the vendor's or project's own published documentation.
- **[public]** a public page that is not the vendor's documentation (README, release page, third-party write-up).
- **[unchecked]** not verified by me, or verified only against a stale source. The closing section lists these together.

**Two hard limits from `AGENTS.md` that shape Part 2.** The project forbids copying a provider's sign-in token out of its own store into Retinue, and forbids scraping a provider's terminal. There are exactly two exceptions: the usage-bar read (30 October 2026 amendment: read Claude Code's `.credentials.json` and OpenCode's `auth.json`/credential database, read-only, send each only to that provider's own usage endpoint, never store or log it) and cloud dictation (12 and 14 October 2026 amendments). Neither covers sign-in. The account design in Part 2 therefore never holds a token: it drives each provider's own login command and lets the provider write its own credential into its own folder. Where a vendor's design would need more than that, the report says so instead of inventing a workaround.

---

## Summary: the ten findings that matter most

1. **The owner's link is not the project round one read.** `CopilotKit/OpenBot` (read this round at commit `b01ac8c7cfcca170b097c07cb4c2f331934fa0c2`, 11 November 2026) is a TypeScript monorepo: a React app, a Hono server, PostgreSQL, Docker Compose and Kubernetes charts, with agent "harnesses" built as container images. `nightly-labs/openbot` (round one) is an Electron desktop app. The two share a name and nothing else that I could find. **[code]**
2. **The licence question flips in our favour.** `CopilotKit/OpenBot` is **MIT** (`LICENSE:1-3`), so we may reuse its code, with the copyright notice kept, as well as its ideas. The round-one project stays **PolyForm Noncommercial 1.0.0**, ideas only. **[code]**
3. **OpenBot does hold tokens, and that is the part we must not copy.** Its Claude and ChatGPT plan sign-ins end with a token or a whole credential store handed to the app, then kept in a DPAPI/0600 file and re-encrypted in PostgreSQL; agents get a derived proxy credential instead. Retinue's rules forbid that, and Part 2 shows a design that gets the same user experience without it. **[code]**
4. **OpenBot drives the vendor's own login command rather than reimplementing OAuth**, which is the right instinct and the one Retinue already follows: it runs `claude setup-token` inside a throwaway container, and a ChatGPT login program inside another, then reads the result out of a pseudo-terminal. **[code]** Retinue cannot copy the reading half (terminal scraping), but it already has a better-fitting mechanism: `runtime_terminal`, which opens Windows Terminal with a fixed command per provider (`apps/desktop/src-tauri/src/lib.rs:1839-1957`). **[code]**
5. **Retinue already has the account model the owner is asking for**, and it is missing exactly one step. `crates/retinue-daemon/src/accounts.rs` keeps one named account per provider in `<data>/accounts/<id>`, points the CLI at it through `CLAUDE_CONFIG_DIR`, `CODEX_HOME` or `XDG_CONFIG_HOME`+`XDG_DATA_HOME`, binds a bot to it, and reads who is signed in from the CLI's own output. What it does not do is run the sign-in: `account.signIn` returns a PowerShell line for the owner to copy (`accounts.rs:62-76`, `236-243`). **[code]**
6. **The sign-in step is a small, contained change**: give `runtime_terminal` the account's id, have the native shell build the same environment prefix the daemon already builds, and call it from the Accounts page instead of showing a copy box. No token is read; nothing new is reachable from the remote gateway (`account.*` is already `never`, `tiers.json:250-255`). **[code]**
7. **Three providers are on by default today, and five others are found.** Discovery looks for `claude`, `codex`, `opencode`, `qwen`, `cursor` and `grok` (`crates/retinue-runtime/src/providers.rs:85-87`), and every one it finds is offered. There is already a per-agent off switch (`runtimes.disabled`, `main.rs:543-564`, `SettingsSheet.tsx:375-384`), so "the three are always on, the rest behind toggles" is a default list plus a Providers page, not new machinery. **[code]**
8. **Google's Antigravity CLI (`agy`) is a real, drivable CLI, and the best fit Retinue has**: `agy acp` is an ACP server over stdio with sessions, permission requests, steering and model selection, which is the same protocol `retinue-adapter-acp` already speaks; `agy -p --output-format stream-json` is a second, headless path. **[docs]** Its sign-in is a browser or keyring flow owned by Google; the CLI's own `agy acp --login` is the documented in-terminal sign-in. **[docs]**
9. **Two integration reports contradict each other on Antigravity.** Monocode drives Antigravity over ACP through a wrapper binary `agy_acp_server.par` and says `agy` ships no Windows ACP server, and it deliberately does **not** sign in from inside the app: "Authentication stays in the provider's Terminal UI; never start OAuth here" (`antigravityCatalog.ts:80-81`). Google's docs, read today, document `agy acp` and a Windows installer. One of the two is out of date; I could not run either to find out which. **[code]** **[docs]** **[unchecked]** T3 Code, at `d2c25950`, does not mention Antigravity at all. **[code]**
10. **OpenBot's coding-bot craft is worth reading, but its shape is different from ours.** A bot there is a profile plus a harness image plus a container "computer"; ours is a CLI in a project folder. The transferable ideas are the ones about orchestration, not about running a model: a typed hand-off with the four decisions kept on the server, pause enforced where work starts, caps that refuse rather than truncate, tool narrowing by skill, and an audit row naming what started every run. **[code]** **[docs]**

---

## Part 0: what I read, and which OpenBot is which

| Repository | Commit read | Date of that commit | Licence | Who it belongs to |
|---|---|---|---|---|
| `github.com/CopilotKit/OpenBot` | `b01ac8c7cfcca170b097c07cb4c2f331934fa0c2` | 2026-11-11 | MIT (`LICENSE:1`) | CopilotKit |
| `github.com/pingdotgg/t3code` | `d2c25950b4d9183ea342d5d5fc14c326b9e0172a` | shallow clone, 11 Nov 2026 | MIT (round one) | pingdotgg |
| `github.com/hardbeat920/monocode` | `7e5a9e3690d5b96a7d93ebf884397835e57b9b7e` | shallow clone, 11 Nov 2026 | MIT (round one) | hardbeat920 |
| `github.com/nightly-labs/openbot` (round one only) | `712e797` (from `docs/research/competitor-adaptation-2026-11.md`) | 2026-11-10 | PolyForm Noncommercial 1.0.0 | Nightly Labs |

**Are they the same project?** No, as far as the trees show. **[code]**

- Different stacks. CopilotKit's tree is `app/` (React + Vite), `server/` (Hono + Drizzle + PostgreSQL), `worker/`, `toolkit/`, `extension/`, `mobile/`, `desktop/` (Tauri 2), `charts/` (Helm), and sixteen `agent-*` directories that build container images. Round one's report describes an Electron app with `src/backend/` and `src/renderer/` and a checked-in marketplace catalogue (`docs/research/competitor-adaptation-2026-11.md:254`).
- Different histories. CopilotKit's root commit is `40cef156`, 14 August 2026, by David McKay, subject "OpenBot"; 1,076 commits to `b01ac8c`. No commit message mentions Nightly, a fork, or an import. `nightly-labs/openbot` is still live and separate (its `HEAD` is `8b3588d5`; verified with `git ls-remote`).
- Different products. CopilotKit's is "AI coworkers, ready for the enterprise", with channels, routines, audit, SCIM and a separately licensed "CopilotKit Intelligence" service. Nightly Labs' is described in round one as a local Electron workspace for Codex/Claude/Grok agents.

I did not diff the two trees or compare their early history beyond the root commit, so "one forked from the other before either's first commit" remains **[unchecked]**. It would be unusual: the languages, the runtime model and the licence all differ.

**Which one counts.** The owner gave the CopilotKit link, so all of Parts 1 and 4 describe `CopilotKit/OpenBot` at `b01ac8c`. Where round one's project does something different and better, I say so and name it.

**A caution about scope.** CopilotKit's OpenBot is not a local repo-coding tool. It runs agent *harnesses* in containers, gives each bot a browser "computer" instead of a checkout, and reaches models through either an API key or a plan sign-in. So it is a very good reference for accounts, orchestration and governance, and a poor one for "how do I run Claude Code in someone's project folder". Part 4 keeps that distinction explicit.

---

## Part 1: accounts and sign-in in OpenBot

### 1.1 How a person adds an account, per provider

The surface is the **desktop setup window**, not the web app. `desktop/src/ProviderPicker.tsx` renders a list of provider rows, and each row offers one or more ways in. **[code]**

The rows are data, in Rust: `desktop/src-tauri/src/provider.rs:84-160`. Each has `logins: Vec<Login>`, where `Login` is `Plan`, `ApiKey`, `Endpoint` or `Oauth` (`provider.rs:18-30`). The order is the order the screen offers, and a test pins "a plan is offered before a key" (`provider.rs:186-196`). **[code]**

| Row id | Name | Ways in, in order | Notes |
|---|---|---|---|
| `openai` | OpenAI | `plan`, `api-key` | "Sign in with ChatGPT Plus, Pro, Team or Enterprise." |
| `anthropic` | Anthropic | `plan`, `api-key` | Carries a required caution about a separate monthly Agent SDK credit pool (`provider.rs:107-124`) |
| `google` | Google Gemini | `endpoint`, `oauth` | Endpoint preset, or Google OAuth for Gemini API access |
| `xai` | xAI | `endpoint`, `oauth` | Device-authorisation sign-in |
| `unbiased` | Unbiased AI | `oauth`, `endpoint` | Device authorisation that mints an API key |
| `openai-compatible` | Any OpenAI-compatible endpoint | `endpoint` | The escape hatch, deliberately last (`provider.rs:8-12`) |

**There are no accounts per user and no second account per provider.** One deployment holds one model connection; the admin screen says so in a comment: "One today, read from the deployment's configuration; a row each once a deployment can hold more than one" (`app/src/components/admin/controls/model-sections.tsx:71-74`). A second account means a second deployment root, and `saved_intent.rs:41-142` records exactly one saved intent per root. **[code]**

**What happens underneath, per path.** Four mechanisms, all different:

1. **Claude plan** — the vendor's own CLI, run in a container, over a pseudo-terminal.
   - `begin_claude_sign_in` (`desktop/src-tauri/src/main.rs:8392`) picks the `agent-claude-sdk` image "whatever harness the person picked", because that image carries Anthropic's bundled CLI at `/usr/local/lib/python3.12/site-packages/claude_agent_sdk/_bundled/claude` (`plan.rs:34-42`), so nothing has to be installed on the machine.
   - It runs `docker run --pull=never --rm -i -t <image> <bundled cli> setup-token` under a 48×200 pty (`plan.rs:249-278`), reads the consent URL out of the terminal output (`plan.rs:55-207`, `325-328`), and returns it to the window; the native side opens it as well, because "a link somebody can copy is the difference between a stuck screen and a finished sign-in" (`main.rs:8422-8428`).
   - Anthropic's CLI cannot reach its own loopback callback from inside the container, so it falls back to printing a code; the person pastes the code back into OpenBot, and `finish_claude_sign_in` (`main.rs:8440`) types it into the still-running CLI (`plan.rs:332-405`).
   - The CLI prints the token; OpenBot recognises it by prefix (`sk-ant-oat`, `plan.rs:44-49`) and returns it to the window.
   - The code documents two details it learned the hard way: Enter must be `\r` and must be a separate write from the paste, because the CLI turns on bracketed paste (`plan.rs:342-376`); and the output is never put in an error message, because it may hold the token (`plan.rs:443-466`).
2. **ChatGPT plan** — a Python login program handed to the `agent-langgraph-agui` image as an argument, not mounted as a file (`plan.rs:510-540`). No pty, because nothing is typed: the browser's redirect finishes it (`plan.rs:715-725`). The container publishes its loopback callback port as the host's 1455, with a small relay because the published port cannot reach a container's `127.0.0.1` and the vendor refuses a non-loopback bind (`plan.rs:481-508`, `513-523`). What it prints is "the whole store, not the access token", because the access token expires within the hour and the store carries the refresh token (`plan.rs:525-528`).
3. **Google and xAI OAuth** — implemented natively in Rust rather than by an agent CLI: `begin_model_oauth` / `finish_model_oauth` (`main.rs:8539`), with the refreshable record and proxy in `desktop/src-tauri/src/provider_oauth.rs` and the server-side proxy in `server/src/provider-oauth.ts`.
4. **API keys and endpoints** — typed into the window, then written to the vault (section 1.2).

**So: one path launches the provider's own CLI login (`claude setup-token`), one launches a vendor login inside a container and relays its callback, two do real OAuth in the app, and one takes a typed key.** The screen has to know which is which: Anthropic wants a code typed back, ChatGPT finishes itself, so only the first shows a field (`ProviderPicker.tsx:388-394`). **[code]**

### 1.2 Where credentials end up, and who can read them

OpenBot holds credentials in three places, and it does hold them. **[code]**

1. **A vault file for pasted keys and plan tokens.** `desktop/src-tauri/src/vault.rs:1-23`: on macOS and Linux an owner-only file under the deployment root; on Windows **DPAPI** through PowerShell's `ProtectedData`, encrypting to the signed-in user. The value never goes on a command line, because `ps` is readable by every process the person runs (`vault.rs:17-19`). The secret list is by name, and includes `CLAUDE_CODE_OAUTH_TOKEN`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY` and `INTELLIGENCE_API_KEY` (`vault.rs:36-58`); everything else goes to `.env` where a person can read it (`vault.rs:60-73`).
2. **A private OAuth record file for Google and xAI plan sign-ins**, with access token, refresh token, expiry and a `proxyToken` (`server/src/provider-oauth.ts:20-33`). Its reader refuses a symlink, a non-file, a file over 64 KiB, and on non-Windows any group or other permission bits (`provider-oauth.ts:78-96`). Refresh is serialised behind a lock and written atomically through a temp file and rename (`provider-oauth.ts:120-190`).
3. **An encrypted column in PostgreSQL**, for anything the server side needs: AES-GCM with a per-row IV, stored as `{version, iv, ciphertext}` (`server/src/credentials.ts:8-12`, `127-175`), in a `credentials` table whose unique index allows at most one live row per `(kind, provider, key_id)` (`server/src/db/schema/core.ts:425-446`).

**Mitigations that matter for us.** React never receives a token: "The desktop's provider credentials remain in its local deployment; React does not receive access or refresh tokens. The local server refreshes credentials before model calls and saves rotated tokens. Agent processes receive a local proxy credential instead of the provider's refresh token" (`desktop/PROVIDER_OAUTH.md:91-96`). The proxy requires its own bearer and answers 401 rather than leaking the provider's response (`provider-oauth.ts:200-215`). A credential read is by id and kind, never by name from a caller. **[code]**

**Why this is still the opposite of Retinue's rule.** A token that OpenBot holds and renews is a token it could log, back up or expose; the Retinue amendments permit a credential read only for the usage bar, only to that provider's own usage endpoint, holding it only for that request. Part 2's design never gets a token at all, so this part of OpenBot is a reference for *what not to build*.

### 1.3 How several accounts of one provider are kept apart

They are not. There is no per-account config directory, no profile directory and no environment override for an account, because there is one account per provider per deployment. **[code]**

- `CLAUDE_CONFIG_DIR` and `CODEX_HOME` appear in the tree only inside two tests (`agent-claude-sdk/tests/test_prompt_too_long.py`, `agent-crewai/tests/test_main.py`), never in product code.
- The isolation that does exist is between **deployments**: each has a root directory holding its `.env`, its vault, its ChatGPT token store at `.langchain/chatgpt-auth.json` (`desktop/src-tauri/src/env.rs:38-39`), and its own containers.
- The isolation that exists *inside* a deployment is between **people and bots**, not accounts: each person gets a computer container, and each bot gets a folder and browser profile in it (`docs/architecture.md:105-112`). Model credentials are deployment-wide.
- A bot is bound to a model connection by the deployment's `BOT_PROVIDER`/`BOT_MODEL`, plus a per-run override. Which harnesses accept that override is decided by a pattern match on the image name (`server/src/agents/harness-model.ts:11-35`).

**For Retinue this is the one place where we are already ahead of OpenBot**, and it is why Part 2 is a small change rather than a large one.

### 1.4 How sign-in state is detected and shown

Three layers, and the third is the one worth copying. **[code]** **[docs]**

1. **At setup, "saved" is a fact about files, not a guess.** `SavedConfiguration` carries `modelSessions` booleans per provider and `modelApiKeys` booleans per vendor (`ProviderPicker.tsx:58-88`). ChatGPT's is read from the token store's presence (`main.rs:8368-8371`), Anthropic's from the vault or the `.env` (`main.rs:8372-8377`). The screen shows a short "Signed in" mark before moving on (`CONFIRMED_FOR_MS = 1000`, `ProviderPicker.tsx:17-18`).
2. **At runtime, a provider refusal is turned into a sentence naming the provider and the one thing to do.** `server/src/agents/provider-refusal.ts` classifies a run's error into `key`, `billing`, `rate_limit`, `model`, `too_many_tools`, `too_long`, `too_large`, `provider_down` or `other` (`:31-45`), reading the status out of four different SDK spellings (`:61-72`) and the provider's own signatures (`:77-81`). The provider's raw text goes to the server log as `model-provider-refused`, not to the screen (`:1-25`).
3. **Expired or wrong-plan sign-ins are their own case.** A signing-in product distinguishes "not signed in" from "signed in but this plan is out of Agent SDK credits" *before* the person needs it: the Anthropic row carries the caution "A Claude plan carries a separate monthly pool of Agent SDK credits. When that pool is empty, Bots stop until it renews or you add API credits", with a link, and a test requires that sentence (`provider.rs:107-124`, `:250-259`).

**Sign-out and removal.** The provider's own revocation is the only real sign-out, and the design says so: "Provider revocation requires signing in again" (`PROVIDER_OAUTH.md:97`). Server-side, `credentialStore.revoke` marks a row revoked and `isLive` refuses it afterwards (`server/src/credentials.ts:104-116`), and a revoked row is refused rather than resurrected on a token refresh (`credentials.ts:69-77`). I did not find a per-provider "Disconnect" control in the app's admin sections; the admin Providers section renders one read-only row (`model-sections.tsx:75-119`). Whether a person can remove a model connection from a settings screen without reinstalling is **[unchecked]**.

### 1.5 Switching account for a running or existing conversation

Not supported, and consistently so. **[code]**

- The model connection is a deployment fact, so there is nothing to switch to.
- What *can* change per turn is the model and effort: the server sends `openbotModel: {model, reasoningEffort}` in the run's forwarded props, and only the harnesses in `TAKES_MODEL` honour it (`server/src/agents/harness-model.ts:11-35`; `agent-claude-sdk/src/adapter.py:189-205` sets `options.model` and `options.effort` from it).
- A bot's *framework* is chosen at creation and kept in its configuration; it cannot be re-pointed later (`server/src/agents/profile-types.ts`, `CreateAgentInput.harness`).
- A harness whose credential is wrong fails the turn with `MODEL_AUTH_REQUIRED`, which the server turns into "sign in again" (`provider-refusal.ts:49-56`).

**The lesson for Retinue is the negative one**: an account is part of what a conversation runs on, so it belongs on the bot (as it already does in `agents.account_id`) rather than on the conversation, and a switch should be a deliberate edit that says what happens to a running turn.

### 1.6 Which providers, which are on by default, how others are enabled

- Six model rows, **all offered, none off by default** (`provider.rs:84-160`). The ordering encodes policy, not availability: named providers first, the custom endpoint last.
- A test enforces the ordering rather than a feature flag (`provider.rs:198-215`), so "which providers" is a compile-time list, not a setting.
- Fifteen agent harnesses plus a bring-your-own-endpoint row, from one file every language reads, `shared/harnesses.json`. Each row names an image, a port, a health path, a `credential` kind (`any-provider`, `anthropic`, `google`, `their-endpoint`) and a `webCreate` flag. Rows the AG-UI table marks "In Progress" are deliberately left out: "offering a harness that cannot answer is worse than a shorter list" (`harnesses.json:_readme`). **[code]**
- There is no per-provider on/off switch anywhere in the product. Retinue's `runtimes.disabled` has no equivalent here.

---

## Part 2: what Retinue should do

### 2.0 What Retinue has today, accurately

**The daemon already models accounts the way this report is going to recommend.** `crates/retinue-daemon/src/accounts.rs` opens with the rule it follows: "An account is a config folder the CLI is pointed at by environment. The default account is the CLI's own folder: it has no row and cannot be deleted. Nothing here reads a token or a credential file; who is signed in comes from the CLI's own output" (`accounts.rs:1-6`). **[code]**

| Piece | Where | What it does today |
|---|---|---|
| Provider list for accounts | `accounts.rs:42` | `["claude", "codex", "opencode"]` only |
| Environment per account | `accounts.rs:47-57` | `CLAUDE_CONFIG_DIR`; `CODEX_HOME`; `XDG_CONFIG_HOME` + `XDG_DATA_HOME` for OpenCode |
| The sign-in text | `accounts.rs:62-76` | Builds `$env:CLAUDE_CONFIG_DIR='...'; claude auth login` (and the Codex/OpenCode equivalents) with PowerShell single-quote doubling |
| Account folder | `accounts.rs:80-83` | `<data>/accounts/<id>`, id restricted to ASCII alphanumerics and `-`, so a delete can never escape |
| `account.create` | `accounts.rs:181-198` | Inserts the row, creates the folder, applies key permissions (`remote::native::apply_key_permissions`), then reads the account in the background |
| `account.signIn` | `accounts.rs:236-243` | **Returns the command string. Runs nothing.** |
| `account.check` | `accounts.rs:128-147` | Reads the provider's own answer for that account and caches `{label, plan, signedIn}` in memory |
| `account.list` | `accounts.rs:166-180` | Lists rows with the cached answer, never running a CLI |
| `account.rename`, `account.delete` | `accounts.rs:199-235` | Rename; delete needs `confirm: true`, refuses while a turn runs on one of its bots, sends the bots back to the default, and removes only a folder under `<data>/accounts` |
| Bot binding | `accounts.rs:150-162`, `crates/retinue-storage/src/phase5.rs:15-31` | `agents.account_id`, nullable, and an account must belong to the bot's own provider |
| Spawn environment | `accounts.rs:365-373` (test) | The account's variables reach `exec_options_for_session`, and two accounts never share a provider process |
| Storage | `crates/retinue-storage/src/phase5.rs:13-46` | `accounts(id, provider, name, created_at)` and `agents.account_id`; "Nothing in this table is a secret" (`phase5.rs:6-7`) |
| Who is signed in | `crates/retinue-runtime/src/account.rs:1-58` | `claude auth status` (JSON), Codex app-server `account/read`, `opencode auth list --format json`; "This module has no file access at all" (`account.rs:5-6`) |
| Usage bars | `crates/retinue-daemon/src/quota_sources.rs:12-16, 51-56, 69-76` | The owner-approved read: `<CLAUDE_CONFIG_DIR>/.credentials.json`, and OpenCode's `auth.json` / read-only credential database |
| Remote gateway | `apps/desktop/src/remote/tiers.json:250-255` | `account.*` is class `never`: "Accounts are sign-ins on the host; only the owner changes them, at the desktop" |

**The UI.** Settings has an **Accounts** page (`apps/desktop/src/ui/AccountsPage.tsx`), reached from `settingsIndex.ts:76-77` and listed in `SETTINGS_PAGE_NAMES` (`settingsIndex.ts:5-8`). It groups by discovered provider, shows the default account line, lists named accounts with "Sign in"/"Sign in again" (`AccountsPage.tsx:82`), and on that button shows a copyable command: "Run this in a terminal, then check again" (`AccountsPage.tsx:70-73`, `87-91`). It also offers a "Check now" button (`:83`), rename, delete with a confirm dialog that names what happens to the bots (`:92-93`), and a Custom models block (`:169-174`). **[code]**

**Onboarding** lists only Claude Code, Codex and OpenCode, in that order and hardcoded (`apps/desktop/src/ui/FirstRunOnboarding.tsx:144`), which already matches the owner's "three always on" decision. It offers `SignInHelp` when a provider is missing or signed out (`:163-164`). **[code]**

**`SignInHelp` is the mechanism to extend.** It calls `runtimeTerminal(key, mode)` — a native command that opens Windows Terminal (`wt new-tab`) or a PowerShell console with `CREATE_NEW_CONSOLE`, running a command from a **fixed table in the native shell**: `echo 'Retinue: at the Claude Code prompt type /login...'; claude` for Claude, `codex login`, `opencode auth login` (`apps/desktop/src-tauri/src/lib.rs:1839-1957`). Its own words: "Only the provider and action come from the renderer; the command text itself is always ours" (`lib.rs:1938-1940`). Then the person presses "I've signed in — check again", which clears the runtime's auth state (`SignInHelp.tsx:45-53`). **[code]**

**A bot's account is chosen in its own settings**: `BotAccess.tsx:22-48` renders an "Account" select bound to `agent.accountId` and saves through `agent.update`. **[code]**

**The gaps, stated plainly.**

1. `account.signIn` does not sign anyone in. It hands back a command line. That is the whole of what the owner is asking for.
2. The in-app path (`SignInHelp` → `runtime_terminal`) only exists for the **default** account, because the fixed table has no environment. So the two mechanisms are split: the default account can be signed in from the app; a named account can only be signed in by copying text into a terminal by hand.
3. Nothing in the UI distinguishes "an account that has never been signed in" from "signed in": `account.list` reads a memory cache that is empty until a check runs, so an unchecked account shows `signedIn: false` with no label (`accounts.rs:13-19`). That is honest but reads as "broken" on the Settings page.
4. There is no provider on/off page; the switch that exists lives under **Agents** and covers coding agents generally (`SettingsSheet.tsx:375-384`), not "Providers".

### 2.1 A design that stays inside `AGENTS.md`

**The shape.** Adding an account creates a folder and points the CLI at it (already done). Signing in **runs that provider's own login command in a visible terminal, with that account's environment set, and lets the provider write its own credential into its own folder.** Retinue then asks the provider's own status command whether it worked (already done, per account). No token is read, copied, parsed, stored, logged or sent anywhere.

That satisfies both prohibitions, and it satisfies them in the way the rules ask for rather than by a narrow reading:

- **No copied provider authentication.** The credential never crosses into Retinue: it is written by the CLI, into a folder Retinue owns, and only the CLI ever opens it. The one existing read of a credential file (`quota_sources.rs`) is untouched and remains limited to the usage bar.
- **No scraping the provider's terminal.** Retinue starts the command and does not read its output. The person reads the terminal, because the terminal *is* the provider's interface. This is materially different from what OpenBot does (`plan.rs:55-207`, `251-405`, which strips escape codes from the TUI and pattern-matches the URL, the token and the refusal), and it is the part of OpenBot's design we must not copy.

**Per provider.**

| Provider | Login command Retinue runs | Variables that point it at one account folder | Status command Retinue already uses | Confidence |
|---|---|---|---|---|
| Claude Code | `claude auth login` **[docs]** ("Sign in to your Anthropic account. Use `--email`…, `--sso`…, `--console`…", `code.claude.com/docs/en/cli-reference`) | `CLAUDE_CONFIG_DIR` **[docs]** ("To keep the home-directory files somewhere else, set `CLAUDE_CONFIG_DIR`; Claude Code then stores your settings, session history, and plugins there instead", `code.claude.com/docs/en/settings`; on Windows `~/.claude` means `%USERPROFILE%\.claude`) | `claude auth status` **[docs]**: JSON, exit 0 signed in / 1 not, `authMethod` one of `none`, `claude.ai`, `oauth_token`, `api_key`, `api_key_helper`, `third_party`, and **`configDirectory` since v2.1.268**, which is the field that proves the account folder is the one being used | High for the command and the variable; **medium for the credential**, see 2.5 |
| Codex | `codex login` (ChatGPT) **[docs]**; `codex login --with-access-token` and `CODEX_API_KEY` exist for automation **[docs]** | `CODEX_HOME`, default `~/.codex` **[docs]**: "Sets the root for Codex state, **including config, auth, logs, sessions, skills**, and standalone package metadata. If you set it, the directory must already exist." | Codex app-server `account/read` over stdio (`account.rs:202-233`) | High: the vendor names `auth` explicitly, and the "must already exist" requirement is already met by `account.create` |
| OpenCode | `opencode auth login` **[docs]** (`opencode.ai/docs/cli`) | `XDG_CONFIG_HOME` and `XDG_DATA_HOME` (what Retinue sets, `accounts.rs:52-55`); OpenCode's global config is `~/.config/opencode/opencode.json(c)` and the data directory holds `auth.json` **[docs]** (`opencode.ai/docs/config`) | `opencode auth list --format json` (`account.rs:54`) | Medium: the docs give `~/.config/opencode` but **never mention XDG**, so honouring the variable on Windows is unproven (2.5) |

**Three details that make it work.**

- **Create the folder first, and let the provider own it.** `account.create` already creates `<data>/accounts/<id>` and restricts it (`accounts.rs:187-189`); Codex documents that it will refuse a `CODEX_HOME` that does not exist, so order matters and the existing order is already right.
- **Use `claude auth login` rather than `claude` + `/login` for a named account.** The current fixed table starts an interactive session and prints a hint to type `/login` (`lib.rs:1841`). That is fine for the default account, where a session may be wanted anyway; for a named account, `claude auth login` is the documented subcommand that signs in and exits, so the terminal window closes on its own and the person is not left inside a session that belongs to nobody.
- **Sign out with the provider's own command, in the same terminal.** `claude auth logout` **[docs]** and the equivalents do the right thing inside the account folder. Deleting the account (which Removes the folder, `accounts.rs:225-233`) remains the heavier option and already says what happens to the bots. Do **not** add an RPC that deletes a file inside the folder by name: the folder belongs to the provider, and the provider's own logout is the operation that knows what it wrote.

**What is deliberately not done.** No reading of `~/.claude/.credentials.json`, `$CODEX_HOME/auth.json` or OpenCode's credential table for the sign-in flow; no OAuth client of our own for Claude or Codex; no "paste your token" field; no attempt to make the CLI believe it has a browser callback it cannot have. Each of those would be a smaller amount of code and a larger amount of rule.

### 2.2 The three ways to start that login, and which to take

| Option | What it is | Cost | Risk |
|---|---|---|---|
| **A. Terminal window, account environment added** (recommended) | Extend `runtime_terminal` to take an `accountId`; the native shell builds the `$env:` prefix from the same table the daemon uses and runs the fixed login command | Small: one command signature, one table mirrored (or read from the daemon), one button in `AccountsPage.tsx` | The `$env:` prefix must come from the shell's own table or from the daemon, never from the renderer (the existing rule at `lib.rs:1938-1940`). Two copies of the environment table would drift: `accounts.rs:47-57` and the native shell. Give it one home |
| **B. An in-app terminal** | Reuse the terminal dock (`apps/desktop/src-tauri/src/pty.rs:1-5`, "Native terminal sessions for the dock") to host the login | Medium: the dock's contract today is "a short list of shells, started inside a project folder it names", so sign-in needs a second, narrower contract; plus UI and a11y work in both looks | It is still a terminal, so the person still reads it; the gain is cosmetic, and the pty surface would now run commands outside a project |
| **C. Daemon starts the CLI headless and reads the URL** | Spawn `claude auth login`/`codex login` with pipes and parse the link | Looks small | **Rejected.** It is terminal scraping by any reading, the output can carry a token, and OpenBot measured that a CLI given pipes rather than a pty writes nothing at all (`plan.rs:246-248`). This is the option that would need the owner to change a rule, and it is not worth it |

**Recommendation: A**, with the copyable command kept as the fallback it already is (it is genuinely useful when a terminal cannot open, and it is what happens today). B is a later, separate decision if the owner wants the sign-in to feel like part of the app; it changes no rule, only the surface.

**The flow, as a person experiences it.** Settings → Accounts → *Add account* (name, already built) → the new row says "Not signed in" → **Sign in** opens a terminal titled "Retinue: Claude Code sign-in — Work" with the account's folder already in its environment → the person finishes in the browser → they press **I've signed in — check again**, or the page polls `account.check` for 90 seconds → the row shows the email and plan the CLI reports, or "Not signed in" and no claim.

### 2.3 How it attaches to what exists

**Daemon and storage.** Very little changes, and that is the point:

- `crates/retinue-daemon/src/accounts.rs`: keep `account.create`, `check`, `list`, `rename`, `delete` as they are. `account.signIn` keeps returning the command text (the fallback and the copy box) and gains nothing else. If a launch is to be daemon-driven, add `account.launch` instead of changing `signIn`, so an old client keeps working.
- `crates/retinue-storage/src/phase5.rs`: no schema change. An account is still `(id, provider, name, created_at)` and a bot still points at it with a nullable `account_id`.
- `crates/retinue-runtime/src/providers.rs`: **the provider list is the thing to widen**, not just `accounts.rs:42`. `discovery_ids()` is a fixed array of six (`providers.rs:85-87`) and the ACP rows carry their own `sign_in_command` (`:30-60`). A seventh provider (Antigravity, Part 3) belongs in one of those two places.
- `crates/retinue-runtime/src/account.rs`: needs a row for any new provider, or it returns `None` and the account shows "not signed in" for ever (`account.rs:50-58`).

**Native shell (Tauri).**

- `apps/desktop/src-tauri/src/lib.rs:1891-1957`: add `accountId: Option<String>` to `runtime_terminal`; validate it with the same character rule as `accounts::account_folder`; resolve the folder from the data directory the shell already knows; prefix the fixed script with the account's `$env:` assignments. Refuse an unknown provider or a malformed id with a message, as the table already does.
- The environment table must not be duplicated by hand. Either the shell asks the daemon for the account's environment through the existing capability-guarded API, or the table is generated from one Rust constant shared by both crates. The second is cheaper; the first keeps one runtime source of truth.

**RPCs.**

- Added: **none required** for the three core providers. `account.create`, `account.signIn`, `account.check`, `account.list`, `account.rename`, `account.delete` cover the flow, and the terminal launch is a native command rather than a daemon method.
- If the owner later wants the daemon to launch it (`account.launch`), that is one method with the same contract, and it is the version that would work from a future Linux build without a Tauri shell.
- Worth adding regardless: `account.check` should be callable in a loop without harm (it is: it reads, caches and returns), and `account.list` should say `checkedAt` so the page can show "last checked 2 minutes ago" instead of implying a live answer.

**The remote gateway.** Nothing new may be reachable from it:

- `account.*` is already class `never` with the reason "Accounts are sign-ins on the host; only the owner changes them, at the desktop" (`tiers.json:250-255`), and a parity test fails the build when a daemon method has no row. A new `account.launch` must be added there as `never`, with its own reason, or the test will stop it.
- Every native Tauri command is outside the gateway by construction; `runtime_terminal` is not in `tiers.json` and must not be.
- The `$env:` text and the folder path never travel: the gateway refuses `account.*` outright, and the sign-in command is only ever composed host-side.

**Where it appears in the UI.**

1. **Settings → Accounts** (`AccountsPage.tsx`): the row's "Sign in" button calls the native terminal with the account's id; the copy box becomes the fallback behind the same button ("Open a terminal" / "Copy the command"). A third state on the row — *not checked*, *signed in*, *signed out* — removes gap 3 in 2.0. Keep the existing "Check now" and the poll after sign-in.
2. **Onboarding** (`FirstRunOnboarding.tsx:144-168`): unchanged. Accounts are a second-account feature, and onboarding's job is the first sign-in, which `SignInHelp` already covers for the default account. One small gain: if the owner wants, the same native command can be given no account id, which keeps one code path for both surfaces.
3. **Hire dialog and a bot's settings**: `BotAccess.tsx:40-48` already binds a bot to an account, but only lists accounts that exist. Two improvements: show each option's signed-in state (a bot hired onto a signed-out account fails its first turn), and offer "Add an account" from the select when the list is empty, instead of sending the person to Settings. The hire dialog itself should not grow a sign-in flow: hiring is already long, and the account is a detail that can be changed later.
4. **The usage-limit line's "switch account"**: the limit line is built in `App.tsx:2788-2792` ("Goes out when the usage limit resets, at …") with the companion note at `App.tsx:4324`. A "Switch account" action there is a good idea and a *separate* one: it is the one place where an account change affects a conversation that already exists, so it needs its own rule — for example, it changes the **bot's** account for the next turn, says so plainly, and never pretends the waiting turn moved. Retinue has no mechanism for changing an account mid-conversation today; OpenBot has none either (1.5), so this is new ground for both.

### 2.4 Providers behind toggles

**Every provider the code knows today.** Six are discovered, plus custom launchers:

| Provider | Id | How it is discovered | Adapter | Sign-in command in the table |
|---|---|---|---|---|
| Claude Code | `claude` | `resolve_command("claude")` on PATH, then version and `claude auth status` (`retinue-runtime/src/lib.rs:226-258`, `:301-308`) | `retinue-adapter-claude` | `claude` then `/login` (`lib.rs:1841`) |
| Codex | `codex` | PATH, then the app server's `account/read` | `retinue-adapter-codex` | `codex login` (`lib.rs:1842`) |
| OpenCode | `opencode` | PATH, then a stored-credential read and a sign-in probe against its background service (`providers.rs:1-6`) | `retinue-adapter-acp` | `opencode auth login` (`lib.rs:1843`) |
| Qwen Code | `qwen` | PATH + a `--help` probe for its ACP token (`providers.rs:31-39`) | `retinue-adapter-acp` | `qwen` |
| Cursor CLI | `cursor` | PATH (`agent`), ACP probe (`providers.rs:40-49`) | `retinue-adapter-acp` | `agent login` |
| Grok CLI | `grok` | PATH, ACP probe (`providers.rs:50-59`) | `retinue-adapter-acp` | `grok login` |
| A launcher the owner wrote | `runtime.profile.*` | Stored, not discovered | whichever the command speaks | none; the owner supplies it |
| Gemini CLI | — | Removed 3 November 2026 by owner decision (`providers.rs:10-11`) | — | — |

**What a Providers page needs.**

*In the daemon.*

- **A default-off list that survives a settings round trip.** Today `runtimes.disabled` is read as an empty array when absent (`main.rs:548-551`), so "nothing is switched off" and "this install predates toggles" look identical. A Providers page where the extra five start off needs to tell those apart once: store a marker such as `providers.defaultsApplied: 1`, and on its absence write the extra ids into `runtimes.disabled` without touching a list the owner has already edited.
- **A refusal, not a surprise, for the three core providers.** If `runtimes.disabled` names `claude`, `codex` or `opencode`, either ignore it for those ids or refuse the write at `settings.set`; silently letting a stored list switch off a provider the owner called always-on would break hiring, onboarding and the usage bar at once. `valid_disabled_runtimes` (`main.rs:543-547`) is where that check belongs.
- **A stable provider row to render.** `runtime.list` already carries `provider`, `authState`, `account` and `disabled`; a small `provider.list` would let the page render a provider that is *not installed* as a row with an install hint, instead of only showing what discovery found.

*In the UI.*

- A new **Providers** page (`settingsIndex.ts:5-8` for the page name and the valid-page list, plus index entries so search finds it), with one row per provider: brand mark, name, what it is for, installed version or "not installed", signed-in state, and a switch.
- The switch reuses `setRuntimeOff` (`SettingsSheet.tsx:378-384`); for the three core providers it is either absent or shown on and not adjustable, with a sentence saying why — not an error after the attempt.
- **Settings → Agents** keeps the existing per-install switch (`settingsIndex.ts:73`), which is about one copy of a CLI rather than about a provider; the two pages must not disagree, so the Providers page should say that a switched-off provider also disappears from hiring.
- **Settings → Accounts** should group by enabled providers only, so a person is never invited to sign in to something they have switched off.
- The **hire dialog** and `BotAccess` should list only enabled providers, and say where the others went ("Claude Code is switched off in Settings → Providers").

**Why the three are always on is also an engineering argument.** Onboarding hardcodes them (`FirstRunOnboarding.tsx:144`), the usage bar knows their quota sources (`quota_sources.rs`), the usage tips accept only those three ids (`usageTips.ts:28`), and the doc-generation and steering paths branch on them. Making them switchable would mean teaching four subsystems about absence; keeping them on keeps that code honest.

### 2.5 What could still go wrong, and the checks that settle it

The design is small, and it rests on three vendor behaviours I could not test on this machine. Each is cheap to check before building, and each has a fallback.

| Risk | Why it matters | Check | If it fails |
|---|---|---|---|
| `CLAUDE_CONFIG_DIR` moves the *credential*, not only settings and history | The docs sentence names settings, session history and plugins; Retinue's own usage code assumes `.credentials.json` lives under the account folder (`quota_sources.rs:51-56`). If it does not, two Claude accounts would share one sign-in | Create an account, run `claude auth status` with the account's environment and read `configDirectory`; then sign in and list the folder | Keep named accounts for Claude out of scope until Anthropic documents it; the variable is still right for settings, and the usage bar keeps reading the default folder |
| OpenCode's XDG handling on Windows | `opencode.ai/docs/config` documents `~/.config/opencode` and never mentions `XDG_CONFIG_HOME`; if the Windows build ignores it, `account.signIn` for OpenCode points at a folder OpenCode will not use | Create an account, run `opencode auth list --format json` with `XDG_CONFIG_HOME`/`XDG_DATA_HOME` set to the folder, and confirm it reports nothing while the default account reports a key; then sign in and confirm the folder gained `auth.json` | Fall back to the provider's own variable if one exists (unconfirmed), or mark OpenCode accounts as "one account only" in the UI and say why |
| A login command that needs a console the OS terminal cannot give it | `claude auth login` and `codex login` are expected to be fine in Windows Terminal; an unknown one may want a TTY it does not get | Run each in `wt.exe` and in the `powershell -NoExit` fallback, on a machine with no prior sign-in | The copy box remains, which is today's behaviour; nothing regresses |
| Two tables of environment variables drifting | `accounts.rs:47-57` and the native shell would each hold one | A unit test in the shell's table against the daemon's, or one shared constant | One shared constant in `retinue-runtime`, imported by both |

---

## Part 3: Google's Antigravity CLI

Antigravity is Google's agentic coding product: an app, a CLI, IDE extensions and a Python SDK, all sharing one authentication layer. **[docs]** The CLI is the part that matters to Retinue.

### 3.1 What it is, and how it is installed

- **Binary:** `agy`, CLI version line "v1.2+" on the quickstart page (the page also gates some flags at v1.3.5 and v1.4.4, so versions move fast). **[docs]**
- **Install, macOS/Linux/Googlebook:** `curl -fsSL https://antigravity.google/cli/install.sh | bash`; default binary `~/.local/bin/agy`. **Windows** has a PowerShell tab on the same page which I could not expand; the troubleshooting page gives the binary location as `C:\Users\<username>\AppData\Local\agy\bin` and shows a PATH line pointing at `C:\Program Files\Google\antigravity-cli`. The two disagree, which is worth knowing before writing an install hint. **[docs]**
- **First launch** is interactive even before any prompt: colour scheme, rendering mode (alt-screen or inline), and a workspace-trust confirmation. **[docs]**
- **It updates itself.** A background updater takes an advisory lock inside `~/.gemini/antigravity-cli/updater/`; `AGY_CLI_DISABLE_AUTO_UPDATE=true` turns it off. A hanging updater is the documented cause of a CLI that will not start. **[docs]**
- **Surfaces share one sign-in:** the app, the CLI, the IDE extensions and the SDK. "Signing in on one local surface shares your credentials across your local environment." **[docs]**

### 3.2 Its sign-in, and where the sign-in is kept

Four ways in, all documented on the Authenticate page. **[docs]**

| Method | How it works | What it leaves behind |
|---|---|---|
| Personal Google account (`oauth-personal`) | Browser OAuth; Free, Pro or Ultra plan | A token profile in the **OS keyring** |
| Gemini Enterprise (`oauth-business`) | Browser, "Business account" → Google Cloud, license selector, project and region | Same, plus `gcp.project`/`gcp.location` in settings |
| Gemini API key (`gemini-api-key`) | `modelProvider: "gemini"` in `~/.gemini/antigravity-cli/settings.json` plus `GEMINI_API_KEY` in the environment | No session at all: "the CLI never establishes an account session" |
| Application Default Credentials | `gcloud auth application-default login` plus `AGY_ADC_AUTH=true` | Google's ADC file, not Antigravity's |

**The keyring is the answer to "where does it keep the sign-in".** "When launching `agy` on your local machine, the CLI attempts to access your operating system's native secure keyring (such as Apple Keychain, Linux Secret Service/dbus, or **Windows Credential Manager**). If a valid token profile is found, the CLI authenticates your session silently without opening a browser." If not, it opens the browser itself. Over SSH it prints a URL and takes a code back. `/logout` "purge[s] saved authentication profiles from your operating system's keyring". **[docs]**

Two consequences for us, both good:

1. **Retinue must never touch that keyring.** It is the provider's store, and reading or copying it would be exactly what `AGENTS.md` forbids. Nothing in this report proposes it.
2. **Account isolation is the open question.** There is no documented variable that points `agy` at a different config folder or a different keyring profile. Settings live at `~/.gemini/antigravity-cli/settings.json`, logs at `~/.gemini/antigravity-cli/log/`, and the keyring is per OS user. "Token profile" is used in the plural, which hints that the CLI can hold more than one, but no documented way exists to choose one per process. **Supporting several Antigravity accounts on one machine is therefore unproven** (2.5-style check below).

An important footnote in the API-key path: setting `GEMINI_API_KEY` alone "has no effect"; `modelProvider` must also be `gemini`. So an API-key account is a **settings-file** decision as well as an environment one, which suits Retinue's folder-per-account model only if the settings file itself can be relocated — unverified.

### 3.3 Modes an app can drive

Antigravity exposes **two** machine interfaces, and both are structured rather than terminal-shaped. That is unusual and welcome. **[docs]**

**ACP — `agy acp`.** "Antigravity CLI includes an ACP server, `agy acp`, so any editor can run it… talks JSON-RPC over `stdin` and `stdout`."

- The client sends `initialize`, then `session/new`, then `session/prompt`; permission requests come back as `session/request_permission`, and the client answers allow or deny.
- Sessions can be loaded, resumed, listed and closed; multi-root workspaces can send extra folders.
- **Steering is supported**: "Sending a prompt while a turn runs steers that turn instead of starting a new one."
- Model and thinking level are exposed as session options (thinking level `Low`…`Max`); third-party models are limited by client.
- `agy acp --login` is the documented in-terminal sign-in ("Sign in with Google interactively in the terminal by pasting a code, then exit. Works over SSH"). With no credentials, requests fail with error `-32000` "Authentication required", and the client may ask the person to pick an auth method, including `terminal-login`.
- Flags after `acp`: `--add-dir`, `--agent`, `--sandbox`, `--dangerously-skip-permissions`, `--process-per-session` (needed for HTTP MCP servers the editor configures), `--ide-mcp-port`, `--log-file`, `--disable-telemetry`.
- ACP logs land in `~/.gemini/antigravity-cli/log/acp-*.log`.

**Headless — `agy -p`, with JSON.** For a single turn or a scripted sequence:

- `agy -p "…" --output-format json` returns one envelope: `conversation_id`, `status`, `response`, `duration_seconds`, `num_turns`, `usage{input,output,thinking,cache_read,total}`, and `structured_output` when `--json-schema` is given.
- `--output-format stream-json` emits NDJSON: one `init` event carrying `cwd`, `tools` and `permission_mode`, then `step_update` events (with `tool_info{name,parameters,output,error}` on tool steps, `subagent_info` on subagent steps), then one `result`.
- **`--input-format stream-json` keeps one process alive across turns**, one `result` per turn, which is the shape an app wants for a long conversation: it skips startup cost and keeps the conversation warm.
- `status` values: `SUCCESS`, `ERROR`, `CANCELED`, `INTERRUPTED`, `INVALID`, `WAITING`, `RUNNING`. Diagnostics go to stderr and the answer to stdout, deliberately.
- **Approvals it cannot get are soft-denied**: the run continues, exits 0, and prints a notice naming the tool and how to allow it. `permissions.allow` rules like `command(git)` and `write_file(src/)` pre-grant specific actions; `--dangerously-skip-permissions` grants everything. An unknown `--model` exits 1 rather than silently falling back.
- `--print-timeout` bounds a run (default: none).

**What an app cannot do headlessly.** Slash commands answered by the CLI itself, including `/usage` and `/model`, "produce a text report rather than a standard event stream" and cannot be sent into a stream-json session; the docs' fix is to run them as a standalone `agy -p /model`. So `/usage` is reachable at best as `agy -p /usage`. Whether that prints a parseable report is **unchecked** — I could not run the binary.

### 3.4 Usage and limits

- `/usage` (alias `/quota`) "Display[s] model quota usage"; `/credits` shows "remaining G1 credits and purchase links"; `useG1Credits` in settings spends AI credits after the plan's quota runs out and is unavailable to business accounts. **[docs]**
- The headless envelope carries **token counts, not quota**: `usage.input_tokens` and friends. That is per-run consumption, not "how much of the plan is left". **[docs]**
- There is no documented usage endpoint, no `agy usage --json`, and no status-line field that reports quota. **[docs]**
- Conclusion: **a usage bar for Antigravity is not available today.** The honest design shows unknown, which is what Retinue's rules already require ("shows unknown as unknown", `AGENTS.md`).

### 3.5 More than one account

- No documented per-process way to choose a credential: the keyring holds the profile, keyed by OS user; settings are a single folder under the home directory. **[docs]**
- "Token profiles" (plural) suggests several can exist, and the enterprise path explicitly says switching project or region means logging out and in again — which reads as one active profile at a time. **[docs]**
- `GEMINI_API_KEY` plus `modelProvider` is per settings file, so if `~/.gemini` can be relocated by an environment variable, API-key accounts could be separated. No such variable is documented. **[unchecked]**

**For Retinue: ship Antigravity as a single-account provider** (the default account, the machine's own sign-in), and say so on the Providers row. A named-account folder for it would be a claim we cannot keep.

### 3.6 How other integrations handle it

**Monocode (`7e5a9e36`) drives Antigravity over ACP, and deliberately leaves the sign-in in the terminal.** **[code]**

- A whole provider lives in `src/integrations/harness/providers/antigravity/`: `antigravity.ts` (30 KB, the session state machine), `antigravityProtocol.ts` (ACP message mapping), `antigravityCatalog.ts` (model discovery), plus live, real and soak tests.
- It spawns a binary through its own ACP client (`antigravity.ts:389`, `:465`) with timeouts that read like hard-won knowledge: 12 s to initialise, 45 s to create a session, 30 minutes for a prompt, and a 120 s stall notice rather than a kill, because "Antigravity answers session/prompt only after its post-turn work… so a wedged server never resolves it" (`antigravity.ts:80-90`).
- Its client capabilities deliberately claim no filesystem and no terminal (`antigravity.ts:96-104`), and claim `session.configOptions` so the agent does not omit model selection.
- **The binary is not `agy`.** `resolve_antigravity` looks for `~/.local/bin/agy_acp_server.par` and `~/.local/share/agy-acp/agy_acp_server.par`, calls the `.par` a "POSIX self-extracting archive", and returns nothing on Windows: "Antigravity ships no Windows ACP binary, so report the provider unavailable there" (`src-tauri/src/harness.rs:2522-2540`). Overrides are refused on Windows too (`:2115-2119`), and its Linux args are a single `--uid=` (`:147-153`).
- **Sign-in is the person's job, and the code says so twice:** the catalog probe is annotated "Authentication stays in the provider's Terminal UI; never start OAuth here" (`antigravityCatalog.ts:80-81`), and every auth-shaped error is rewritten with `Run \`agy\` once in Terminal to sign in.` (`antigravity.ts:93-101`). Its README instruction is the same: install, "then run `agy` once to sign in" (`docs/providers.md:12-14`).

That is the closest thing to a precedent, and it is a strong argument for Part 2's design: **a serious integration of this exact CLI chose to drive it over ACP and to leave the sign-in to the provider's own terminal.** It also warns us about a trap: monocode's `.par` wrapper exists because, at the time it was written, there was no `agy acp`; Google's docs now document `agy acp` directly. One of the two facts is stale (see 3.8).

**T3 Code (`d2c25950`) does not integrate Antigravity.** A case-insensitive search of every `.ts`, `.tsx`, `.json` and `.md` file, excluding `node_modules` and `.repos`, finds the word only inside a vendored dependency (`alchemy-effect/packages/gcp/src/AIPlatform/Agent.ts`, which is Google's own SDK naming, not an integration). **[code]** So the brief's "T3 Code and Monocode both list Antigravity" holds for Monocode only at this commit; round one's report also read T3 Code and would have seen the same tree.

### 3.7 What supporting it in Retinue would take

**Closest existing adapter: `retinue-adapter-acp`.** Antigravity speaks the same protocol as OpenCode, Qwen, Cursor and Grok, including steering, permission requests and session resume — all of which Retinue's ACP adapter already handles for four providers. This is the cheapest possible new provider: a table row, not a crate.

Concretely:

1. **A row in `crates/retinue-runtime/src/providers.rs`.** `provider: "antigravity"`, `command: "agy"`, `args: &["acp"]`, `probe_token: "acp"` (its `--help` will mention ACP; unverified), `sign_in_command: "agy acp --login"` — which passes the table's own test, since that allows letters, spaces and dashes (`providers.rs:102`), `install_hint` naming the install script. `discovery_ids()` grows from six to seven (`providers.rs:85-87`), and `is_acp_provider` starts answering true.
2. **A signed-in check in `crates/retinue-runtime/src/account.rs`.** There is no `agy auth status`. The structured options are: (a) `agy models` and see whether it lists anything; (b) an ACP `initialize` + `session/new` probe, treating JSON-RPC error `-32000` "Authentication required" as signed out. (b) is more precise and is protocol, not terminal. It belongs beside the existing Codex app-server reader (`account.rs:202-233`), which already does exactly this shape: initialise, ask, close, with a budget and a process-tree kill.
3. **A provider guide in `apps/desktop/src/ui/providerGuides.ts`** — install command, `agy --version`, `agy acp --login`, and the docs links.
4. **Permissions.** Never `--dangerously-skip-permissions` (`AGENTS.md`: a permission mode that is never bypass). Map ACP permission requests to Retinue's approval flow, which the adapter already does; add `--sandbox` only if the owner wants container-ish containment and the platform supports it — the flag is documented as having no effect where the sandbox is unavailable.
5. **Models and effort.** Read ACP session options rather than hardcoding slugs; if the picker wants a static list, `agy models` prints slugs with labels. Third-party models (Claude, per Google's models page) appear only in some clients over ACP — do not promise them in the picker.
6. **Usage.** Show unknown. Do not add a quota source.
7. **Accounts.** Default account only, with a sentence on the Providers row saying why (one keyring profile per OS user, no documented folder override).

**What it costs in the Providers page (Part 2.4).** Antigravity is a seventh provider, so it needs a default-off entry in `runtimes.disabled` and a row with an install hint; nothing else.

**What it costs in the Accounts page.** Nothing if it is single-account: the row shows the default account's line, and "Add account" is hidden for it, as a provider with no isolation story should be.

### 3.8 Unknowns, in the order they would bite

- **Does `agy acp` exist in the build a person installs today?** Google's docs say yes and document its flags in detail; monocode's code says Antigravity ships no Windows ACP server and resolves a separate `.par`. Either monocode is stale, or the Windows build lags the documentation. This is the first thing to test on a real Windows machine. **[unchecked]**
- **Is `agy` on Windows able to run an ACP server at all?** Affects whether Retinue can ship Antigravity on Windows, which is the only platform Retinue targets today. **[unchecked]**
- **Which keyring entry names the Antigravity profile**, and whether two OS users (or two profiles) can hold separate ones. Affects the account story only. **[unchecked]**
- **Whether `agy -p /usage` prints a parseable report.** Would change 3.4 from "no usage" to "usage from one command a minute". **[unchecked]**
- **Whether `~/.gemini` can be relocated** by any environment variable, documented or otherwise. Would change 3.5 entirely. **[unchecked]**
- **The Windows install path and the PowerShell installer.** The quickstart's Windows tab did not render in my fetch, and the troubleshooting page contradicts itself (`%LOCALAPPDATA%\agy\bin` versus `C:\Program Files\Google\antigravity-cli`). An install hint needs a real check. **[unchecked]**

---

## Part 4: how OpenBot runs coding bots

### 4.1 What a bot is there

A bot is three things stacked: **a profile** (name, title, role description, owner, visibility, isolation flag), **a harness** (a container image that speaks AG-UI and wraps an agent framework), and **a computer** (a container with Chromium, a shell, a workspace folder and a browser profile). **[code]** **[docs]**

- The profile is stored in `agents` and `agent_profiles`; "a coworker is a durable Bot profile: `agents` stores name, runtime identity and endpoint/key reference; `agent_profiles` stores title, role, owner, visibility, isolation, and deletion state" (`docs/architecture.md:176-181`). The TypeScript shape is `AgentProfile` in `server/src/agents/profile-types.ts`, whose comments are unusually explicit about what is *not* stored: `hasAuth: boolean` ("Whether a key is set for it. Never the key") and `hasCallbackToken: boolean` ("A boolean, never the token").
- The harness comes from `shared/harnesses.json`: fourteen frameworks plus a bring-your-own-endpoint row, each with an image directory, a port, a health path, a run path, a `credential` kind and a `mark`. The file's own `_readme` explains the ranking (stars first, downloads as a sanity check), the exclusion of anything AG-UI marks "In Progress", and that `image` is the directory rather than the id.
- The computer is the part Retinue has no analogue for: "each person gets a computer container of their own, and every Bot they use has its own folder and browser profile in it; a Bot marked Isolated gets a container of its own instead" (`docs/architecture.md:105-112`). It carries its own token, derived as `HMAC-SHA256(COMPUTER_TOKEN, "openbot computer token v1\n<kind>:<id>")`, filters its own egress against the bot's network policy, and refuses cloud metadata addresses in every mode.

**Compared with Retinue:** our bot is a name, a look, a model, an account and a coding-agent CLI started in a project folder. Theirs is a name, a role description, a framework container and a sandboxed computer. That is a product difference, not a maturity difference: they do not run Claude Code in your repository, and we do not give a bot a browser. It follows that Part 4's borrowable ideas are mostly about **orchestration**, not about running a model.

### 4.2 How a bot is started and kept alive

- **Nothing per bot is a long-running process at the framework level.** Harnesses are compose services (`agent-langgraph`, `agent-agno`, …), and the one the person chose at setup runs behind a `harness` profile as `agent-harness` on port 4202 (`docs/architecture.md:18-30`). A harness is health-checked at `healthPath` and addressed at `runPath`, both from `harnesses.json`.
- **Per-bot state is the computer, not the agent process.** The supervisor "creates, stops, resets, and lists per-Bot computer containers" and exposes only those four operations (`docs/architecture.md:12`, `:96-98`). An idle-computer culler runs on the same queue as routines.
- **A run is a turn sent over AG-UI**, not a process: "the server resolves the signed-in actor and selected coworker… CopilotKit runtime sends the turn to the configured AG-UI endpoint" (`docs/architecture.md:64-70`). Liveness therefore lives in the run, and the harness holds no conversation of its own — conversation is server-side plus Intelligence.
- **Pause and reset are first-class and enforced late.** `server/src/agents/lifecycle.ts:1-20`: "Every place that starts a Bot's turn without a person typing — a routine firing, a responsibility's event, a hop from another Bot, a follow-up the Bot scheduled for itself, every headless turn — asks `isBotPaused`… immediately before it starts. A queue that was filled before the pause therefore drains into skips rather than into turns." And: "FAILS CLOSED. When… the question cannot be answered, the answer is 'paused'."
- **Work that has to happen later is a row on a queue, not a timer**: routines and the culler share `work_items`, "claimed with `select … for update skip locked`, leases timed on the database's own clock, and an attempt cap" (`docs/architecture.md:206-211`), because "a timer fires in every replica".

### 4.3 How it talks to each provider

- **Each harness has its own protocol; AG-UI is the common one.** The harness image wraps a framework (`ag_ui_claude_sdk`, `ag_ui_antigravity`, LangGraph's AG-UI server, …) behind FastAPI with `/health` and a run path.
- **Every harness is authenticated to the server by a shared bearer.** `agent-claude-sdk/src/main.py:37-47` checks `x-openbot-agent-token` against `MANAGED_AGENT_TOKEN` with a constant-time compare, and exempts only `/health`. It also refuses to start when *both* `ANTHROPIC_API_KEY` and `CLAUDE_CODE_OAUTH_TOKEN` are set, because "Anthropic resolves `ANTHROPIC_API_KEY` ahead of `CLAUDE_CODE_OAUTH_TOKEN`, so a container given both uses the key and quietly ignores the subscription somebody chose" (`main.py:7-15`, `24-35`).
- **Flags and permission settings are per harness, and the Claude one is the most instructive.** For a built-in Bot with no filesystem, `agent-claude-sdk/src/adapter.py:652-675` sets `options.tools = []`, `options.setting_sources = []`, `options.strict_mcp_config = True`, `options.permission_mode = "dontAsk"`, one `allowed_tools` rule for its own MCP server, and a `PreToolUse` hook that denies anything not offered: "Built-in Bots may only call tools supplied by the OpenBot server." A second mode keeps `READ_TOOL` and `TOOL_SEARCH` (`adapter.py:692-701`). So a "coding bot" there is a coding-shaped agent with **no shell and no file access** unless it is given the computer's tools — the opposite of Retinue, where the CLI is the whole point.
- **Model and effort travel per run** as `openbotModel`, and only the harnesses that opt in honour it (`server/src/agents/harness-model.ts:11-35`; `adapter.py:189-205`, `586-590`).
- **Antigravity is pinned to Gemini and cannot drift**: "Its SDK drives a bundled Go `localharness`, and that harness wants a Gemini key wherever its base url points… So the provider is pinned here rather than read from the environment, and the catalogue row says `credential: "google"` so the picker asks for the right key" (`agent-antigravity/src/main.py:1-14`). The base URL is the API root with no version path, "and a url carrying the version already gets it twice and 404s, which the harness reports as the unplaceable `agent executor error: Error 0, Message: Not found`" (`main.py:22-30`).
- **A second, model-only endpoint** exists on the Claude harness (`/model` plus `/model/cancel`, `main.py:62-83`), so the product can use the same credential for a small helper call without a second connection.

### 4.4 Workspaces, memory and instructions per bot

- **No checkout, so no project instructions.** `setting_sources = []` and `tools = []` mean no `CLAUDE.md`, no skills from disk, no `AGENTS.md` (`adapter.py:652-656`). What the agent knows is assembled server-side.
- **Per-bot instructions are the role description**, plus an optional `instructions` field reserved for the personal starter — "a Bot a person made runs on its role description, and a package's own Bot keeps its prompt to itself" (`profile-types.ts`, `AgentProfile.instructions`).
- **Per-person standing instructions** are a separate, capped block appended to every prompt: `server/src/user-instructions.ts:1-30` caps it at 4,000 characters, deletes the row when the box is emptied ("One representation of none, and it is absence"), and is explicit that it "is not a grant and it changes nothing about what a coworker may call".
- **Memory is a service, with a fallback**: CopilotKit Intelligence holds content and does recall; OpenBot's own tables keep provenance, review state and which bot formed it; "Without Intelligence memory… OpenBot reads and writes its tables exactly as before and nothing fails" (`docs/architecture.md:370-396`).
- **Workspace, for the computer, is a folder and a browser profile per person and per bot**, plus `/workspace` inside `agent-computer` (`docs/architecture.md:18`, `:105-112`).
- **Skills** are instructions with declared tools (`skill_tools`), and a skill's tools are *offered* only when the message matches it, without ever granting anything: "A declaration grants nothing: the offer is always intersected with what the Bot was already granted" (`docs/architecture.md:350-358`).

### 4.5 How bots hand work to each other

This is the most transferable part of the design, and it is documented as a sequence of decisions rather than a feature. **[docs]**

1. **A hand-off is a typed ask, not a paragraph.** "The asking model names the task, anything that bounds it and what a good answer looks like, rather than writing a paragraph. Free text is the commonest way a handoff goes quietly wrong" (`docs/architecture.md:239-242`).
2. **Four things are decided by the deployment and never by the model**: who is being addressed (resolved against the roster the asking person may see, with a non-enumerating refusal), where the answer lands (from the signed run assertion), who is asking (stamped from the deployment's own row), and how deep the chain is (also from the assertion) (`:244-258`).
3. **The answer lands under the answering bot's name, in the asking conversation, without a second model call** — the deployment writes it word for word with an id naming the bot (`:260-266`).
4. **The addressed bot reads the asking conversation as quoted context, never as its own turns**, because otherwise "a person's 'ask Knowledge, do not answer it yourself' tells Knowledge not to answer" (`:268-271`).
5. **A hop is claimed work on the queue, with a renewed lease**, because "a hop held in memory is lost the moment either is rescheduled" (`:278-282`).
6. **Caps refuse rather than truncate**: `BOT_HANDOFF_MAX_DEPTH`, `BOT_HANDOFF_MAX_PER_RUN`, and `0` switches the capability off entirely — "then no Bot is offered the tool and the delivery loop does not run" (`:284-288`).
7. **A hop that fails for good is said out loud** in the conversation the person is watching (`:273-276`).
8. **Every outcome is audited**, and "the refused row is the one that matters most, because a hop that happened is visible in the transcript and one that was refused is invisible everywhere else" (`:290-292`).
9. **`ask_person` sits beside `message_bot` and is offered to every run**, because stopping to ask "costs nothing and cannot be aimed anywhere they cannot see", and "a deployment able to switch off the safe exit and keep the expensive one would be backwards" (`:294-303`).

### 4.6 What its interface does well for coding work

- **Watching a bot work is three views on one panel**: live browser screen, the bot's one live shell (a persistent `bash`, so `cd` carries over), and an activity list of every command with output and exit code, read from the conversation's own tool results so it survives a reload (`docs/architecture.md:168-172`).
- **Take control is explicit and audited**: `computer.help_requested`, `computer.control_taken`, `computer.control_released`; while a person controls it, bot actions are refused rather than queued (`:154-163`).
- **The handover is the designed answer to a bot check** — ask the person to solve it in the live screen, then hand back — with the page recorded so a restarted computer reopens it.
- **Secrets are entered out of band**: the trail records that a secret was requested or supplied and its character count, never its value (`:166`).
- **An audit row names what started a run** (`person`, `deployment`, `routine`, `responsibility`, `memory`, `handoff`), which is the only way to separate "somebody was watching" from "this happened at 3 a.m." and is filterable in the UI (`:100-128`).
- **Per-provider cautions before the fact**, e.g. the Anthropic credit-pool sentence with a test requiring it (`desktop/src-tauri/src/provider.rs:107-124`, `:250-259`).

### 4.7 Ideas worth borrowing, and where each one lands

Round one already covered OpenBot's queue behaviours (promote-and-interrupt, run-as-task, per-bot busy-send mode) from the *other* OpenBot; none of those are repeated here. Every row below is from `CopilotKit/OpenBot` at `b01ac8c`.

| # | Idea | Evidence there | Retinue today | The gap | Where it attaches | Size |
|---|---|---|---|---|---|---|
| 1 | **Pause is enforced where work starts, and fails closed** | `server/src/agents/lifecycle.ts:1-20` | Automations and delegations start turns from several places; the owner can pause new hand-offs (`settingsIndex.ts:79` lists "Pause new hand-offs") | Pause is a setting read by the caller, so a new caller can forget it. Nothing enforces it at the one choke point | One guard in the daemon beside turn admission, called by automations, delegations, queue drain and sub-bots; unknown means paused | M |
| 2 | **Caps on hops refuse rather than truncate, and `0` removes the tool** | `docs/architecture.md:284-288` | `delegations.rs`, `team.rs` own the ledger; caps exist as settings | A refused hop should be *impossible to attempt* (tool not offered), not merely rejected afterwards, and the refusal should be an audit row | Delegation tool registration + `delegations.rs` | S |
| 3 | **A typed hand-off ask: task, bounds, what a good answer looks like** | `docs/architecture.md:239-242` | The manager's delegation prompt is prose written by a model | Free text is where a delegation goes quietly wrong | The delegation brief template and its validation | S |
| 4 | **The answer lands under the answering bot's name, with no second model call** | `docs/architecture.md:260-266` | Team messages and branch lines exist; whether a delegated reply is re-voiced by the asker is unknown to me | Re-voicing costs a turn and blurs attribution | Team message routing (`team.rs`), reply rendering | M |
| 5 | **Tool narrowing by skill, intersecting with grants** | `docs/architecture.md:350-358` | `skills.rs` and the skill library exist; MCP tools per bot exist | Offering every granted tool on every turn, rather than the matching skill's | Skill selection before a turn starts, in the daemon | M |
| 6 | **`ask_person` offered on every run, beside handing on** | `docs/architecture.md:294-303` | `questions.rs` exists | Verify it is offered regardless of grants, and that its copy says stopping is free | `questions.rs` and the run's tool list | S |
| 7 | **A refused hop is the audit row that matters** | `docs/architecture.md:290-292` | Delegation ledger records outcomes | A refused *offer* (cap, missing grant) may not produce a row | `delegations.rs` | S |
| 8 | **The provider table carries what credential a row needs** | `shared/harnesses.json` (`credential`), `provider.rs:84-160` | `AcpProvider` has `sign_in_command` and `install_hint` but no statement of whether a provider can have several accounts or an API key | Part 2's pages have to guess which providers get an Accounts row and which get a key field | `crates/retinue-runtime/src/providers.rs` (one field), read by Settings | S |
| 9 | **A per-provider caution shown before it bites** | `provider.rs:107-124` + its test | Usage tips exist (`usageTips.ts`), triggered at 75/90/100 % | Nothing warns about a *structural* limit, such as a plan's separate credit pool, before the first failure | The Providers row from Part 2.4, and the usage-limit line | S |
| 10 | **An audit field naming what started a run** | `docs/architecture.md:100-128` | Activity and execution snapshots exist per turn | Separate "a person asked" from "a routine, a hand-off or a memory import did" | The execution snapshot and the activity list | M |

**What not to borrow, and why.**

- **Holding tokens.** Part 1.2. Their whole credential design assumes the app is the credential broker. Ours must not be, and Part 2 shows the cost of not being one is one terminal window.
- **A container and a browser computer per bot.** A large security budget (tokens per computer, egress filtering, policy engine, gVisor option) for a surface Retinue does not have. It would be a new product, not a feature.
- **Full access by default.** Round one already recorded that their default posture is the opposite of ours (`.claude/skills/retinue-development/SKILL.md` golden rule 2, and `docs/research/competitor-adaptation-2026-11.md:287`). Nothing in this repository changes that.
- **The enterprise identity stack** (SCIM, tenant packages, groups). Retinue is a single-owner desktop app; the interesting part is the audit vocabulary, not the provisioning.

---

## Sources

**Repositories read** (cloned outside the repository, under `%TEMP%\opencode\research-openbot`, and deleted at the end of the run; nothing in them was built, run or installed).

| Repository | Commit | Read for |
|---|---|---|
| `https://github.com/CopilotKit/OpenBot` | `b01ac8c7cfcca170b097c07cb4c2f331934fa0c2` (2026-11-11, "Bring OpenBot in front as it first shows after an in-app update on Windows (#1490)") | Parts 1 and 4, plus the licence |
| `https://github.com/hardbeat920/monocode` | `7e5a9e3690d5b96a7d93ebf884397835e57b9b7e` | Part 3.6 |
| `https://github.com/pingdotgg/t3code` | `d2c25950b4d9183ea342d5d5fc14c326b9e0172a` | Part 3.6 (absence check only) |
| `https://github.com/nightly-labs/openbot` | `8b3588d511e6a0991c32f8deb1033bc1f1c82b26` (HEAD, checked with `git ls-remote`; not cloned this round) | Part 0 (identity check only) |

**Files in `CopilotKit/OpenBot` cited above, in the order they appear**

- `LICENSE:1-3`; `package.json` (`"license": "MIT"`); `README.md:107` ("The OpenBot application template is open source under the MIT license").
- `desktop/src-tauri/src/provider.rs` (84-160 catalogue, 18-30 `Login`, 107-124 and 250-259 the Anthropic caution, 198-215 ordering tests).
- `desktop/src-tauri/src/main.rs` (8368-8377 saved sign-ins, 8392-8453 Claude sign-in commands, 8457-8529 ChatGPT sign-in commands, 8539 model OAuth, 11108-11115 registration).
- `desktop/src-tauri/src/plan.rs` (34-49 image and token prefix, 55-207 reading the terminal, 225-236 budgets, 249-329 `begin`, 332-405 `finish`, 443-466 `gave_up`, 476-540 ChatGPT image, store, ports and relay, 715-725 no pty, 738-750 late callbacks).
- `desktop/src-tauri/src/vault.rs` (1-23 storage per platform, 36-73 the secret list and the `.env` split).
- `desktop/src-tauri/src/env.rs` (38-39 ChatGPT store path, 1507-1559 writing and checking it).
- `desktop/src-tauri/src/saved_intent.rs` (41-142 one intent per root).
- `desktop/src/ProviderPicker.tsx` (17-18 the "Signed in" pause, 58-88 `SavedConfiguration`, 306-361 OAuth, 388-441 plan sign-in, 443-470 the code, 472-476 the row list).
- `desktop/PROVIDER_OAUTH.md` (10-97, especially 91-97 credential lifecycle and revocation).
- `server/src/credentials.ts` (8-12 envelope, 69-77 rotatation refusals, 104-116 revoke/isLive, 127-175 AES-GCM).
- `server/src/db/schema/core.ts:425-446` (`credentials` table and its live-row index).
- `server/src/provider-oauth.ts` (20-33 record, 78-96 reader, 120-190 refresh, 200-215 proxy auth).
- `server/src/agents/provider-refusal.ts` (17-56 kinds and harness signals, 61-81 status and signature patterns).
- `server/src/agents/harness-model.ts` (1-35 which harnesses take the model).
- `server/src/agents/lifecycle.ts` (1-20 pause semantics).
- `server/src/agents/profile-types.ts` (`AgentProfile`, `CreateAgentInput`).
- `server/src/user-instructions.ts` (1-30).
- `agent-claude-sdk/src/main.py` (1-47), `agent-claude-sdk/src/adapter.py` (189-205, 586-615, 640-701).
- `agent-antigravity/src/main.py` (1-40).
- `app/src/components/admin/controls/model-sections.tsx` (46-74 one connection per deployment, 75-119 the read-only Providers row).
- `shared/harnesses.json` (`_readme` and rows).
- `docs/architecture.md` (12-30 services, 64-70 run flow, 96-128 computers and initiators, 154-172 control, secrets, watching, 176-198 coworkers, 200-211 routines, 231-323 hand-offs and asking a person, 325-368 tools and skills, 370-396 memory).

**Files in other repositories**

- Monocode: `src/integrations/harness/providers/antigravity/antigravity.ts` (80-104 timeouts and capabilities, 93-101 auth help, 378-410 start, 465 spawn), `antigravityCatalog.ts` (36-81, including 80-81), `src-tauri/src/harness.rs` (147-153 args, 2115-2119 Windows override refusal, 2522-2540 resolve), `docs/providers.md:12-14`.
- T3 Code: no integration; the only match is a vendored `alchemy-effect/packages/gcp/src/AIPlatform/Agent.ts`.

**Retinue files cited above**

Line numbers are from the working tree as it stood on 11 November 2026 (HEAD `e393a7e` plus whatever other lanes had uncommitted at that moment), so they can drift; the symbol and file names are the stable reference.

- `crates/retinue-daemon/src/accounts.rs`, `quota_sources.rs`, `main.rs` (543-564 disabled runtimes), `crates/retinue-storage/src/phase5.rs`, `crates/retinue-runtime/src/account.rs`, `crates/retinue-runtime/src/providers.rs`, `apps/desktop/src/remote/tiers.json` (250-255), `apps/desktop/src/ui/AccountsPage.tsx`, `SettingsSheet.tsx` (375-384), `settingsIndex.ts`, `FirstRunOnboarding.tsx` (144-168), `SignInHelp.tsx`, `BotAccess.tsx` (22-48), `usageTips.ts` (28), `usageLimitSend.ts`, `App.tsx` (2788-2792, 4324), `apps/desktop/src-tauri/src/lib.rs` (1839-1957), `apps/desktop/src-tauri/src/pty.rs` (1-5).

**Documentation read**

| Source | URL | Used for |
|---|---|---|
| Claude Code, CLI reference | `https://code.claude.com/docs/en/cli-reference` | `claude auth login`, `claude auth logout`, `claude auth status` and its JSON fields, `configDirectory`, `--permission-mode`, `--tools`, `--setting-sources` |
| Claude Code, settings | `https://code.claude.com/docs/en/settings` | `CLAUDE_CONFIG_DIR`, and the Windows meaning of `~/.claude` |
| Codex, environment variables | `https://developers.openai.com/codex/environment-variables` | `CODEX_HOME` ("config, auth, logs, sessions, skills", must exist), `CODEX_API_KEY`, `CODEX_ACCESS_TOKEN` |
| Codex, config basics | `https://developers.openai.com/codex/local-config` | Configuration precedence and `~/.codex/config.toml` |
| OpenCode, config | `https://opencode.ai/docs/config/` | `~/.config/opencode/opencode.json(c)`, system directories, the absence of any XDG statement |
| Antigravity, get started (CLI tab) | `https://antigravity.google/docs/getting-started?tab=cli` | `agy`, the installer, `~/.local/bin/agy`, first-run prompts, `-p`, `-i`, `/usage` |
| Antigravity, authenticate (CLI tab) | `https://antigravity.google/docs/install/` | Keyring sign-in, SSH code flow, API key plus `modelProvider`, ADC, `/logout`, the `auth.type` table |
| Antigravity, CLI commands | `https://antigravity.google/docs/cli/reference/` | Every flag, `settings.json` location and keys, permission presets |
| Antigravity, headless mode | `https://antigravity.google/docs/cli/headless/` | `json`/`stream-json` shapes, `--input-format stream-json`, status values, soft-deny of approvals, slash-command limitation |
| Antigravity, other editors (ACP) | `https://antigravity.google/docs/ide/extensions/acp/` | `agy acp`, `--login`, `-32000`, auth method ids, session options, steering, log path |
| Antigravity, CLI troubleshooting | `https://antigravity.google/docs/cli/troubleshooting/` | Windows binary locations, keyring errors, the self-updater lock |

**Prior rounds in this repository**

- `docs/research/competitor-adaptation-2026-11.md` — round one's reading of `nightly-labs/openbot` (licence at `:25`, its Electron shape at `:254`, its default posture at `:287`) and of T3 Code and Monocode's licences.
- `docs/research/ade-ux-and-delegation-2026-11.md` — not used for facts here.

---

## What I could not verify

Each item is something this report relies on or explicitly flags. None of them was checkable without installing or running a provider CLI, which the brief forbids.

1. **Whether the two OpenBots share any early history.** I compared the root commit, the stack and the licence, not the trees. **[unchecked]**
2. **Any live behaviour of `claude`, `codex`, `opencode` or `agy`.** Every statement about what those binaries do at run time is from their documentation, not from running them.
3. **Whether `CLAUDE_CONFIG_DIR` moves Claude Code's credential** (`~/.claude/.credentials.json`) as well as settings, history and plugins. The docs name the latter three only; Retinue's usage code already assumes the former (`quota_sources.rs:51-56`). This is the single most important check before building named Claude accounts.
4. **Whether OpenCode honours `XDG_CONFIG_HOME`/`XDG_DATA_HOME` on Windows.** Its config page never mentions XDG, and Retinue's account isolation for OpenCode depends on it.
5. **Whether `claude auth login` and `codex login` behave in `wt.exe` and in the direct `powershell -NoExit` fallback** exactly as `runtime_terminal` would launch them, including on a machine that has never signed in.
6. **Whether `agy acp` exists in the Windows build a person installs today.** Google's docs say yes; monocode's code says Antigravity ships no Windows ACP binary and resolves a separate `.par`. One is stale. I could not tell which. Related: whether the `.par` wrapper is a community package or Google's own alternative distribution — monocode's code does not say, and I did not find it documented.
7. **Which `--help` token proves an `agy` build can run ACP** (the `probe_token` that a Providers table row would need).
8. **Whether `agy -p /usage` prints a parseable report**, which is the only possible route to an Antigravity usage bar.
9. **How Antigravity's keyring entries are named, and whether two profiles can coexist** for one OS user.
10. **Whether any environment variable relocates `~/.gemini`**, which decides whether Antigravity could ever have more than one account in Retinue.
11. **The exact Windows install command for the Antigravity CLI.** The quickstart's Windows tab did not render in my fetch, and the troubleshooting page gives two different directories.
12. **Whether OpenBot's app has a per-provider "Disconnect" control.** The admin Providers section renders one read-only row (`model-sections.tsx:75-119`); I did not find a removal control, but I did not search every settings surface.
13. **Whether Retinue re-voices a delegated bot's reply under the asking bot's name.** Row 4 of Part 4.7 assumes a gap I did not confirm in `team.rs`.
14. **Whether OpenBot's ChatGPT flow uses LangChain's ChatGPT login or the Codex CLI's.** The store path `/root/.langchain/chatgpt-auth.json` and the image (`agent-langgraph-agui`) point at LangChain, while the relay mentions "the `/cancel` the Codex CLI sends" (`plan.rs:748`); both clients are present in the tree, and I did not trace which one the login program calls.

---

## Proposed skill update (not applied: this run wrote one file only)

`.claude/skills/retinue-development/references/codebase-map.md`, under Docs → Research, one line:

> Accounts and OpenBot (2026-11-11): `docs/research/accounts-and-openbot-2026-11.md` is the reference for in-app sign-in (Part 2: run the provider's own login in a terminal with the account's environment), the per-account model in `accounts.rs`, and the Providers-with-toggles design. It also records that `runtime_terminal` (`apps/desktop/src-tauri/src/lib.rs:1839-1957`) is the existing sign-in mechanism and that Antigravity (`agy acp`) is the cheapest provider Retinue could add.




