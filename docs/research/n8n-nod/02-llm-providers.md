# 02 — How nod supports so many LLM providers

Research only. No code in Retinue was changed. 16 November 2026. Author: Grey (research and planning).

**Source.** `https://github.com/n8n-io/nod`, cloned shallowly to `C:\Users\Sico Vibes\Desktop\DevCN\_research\nod-providers\repo` at commit `47cfee3fe77279e1d543a367131d3ff2c1202e18` (2026-11-16). Links below are permalinks at that commit: `https://github.com/n8n-io/nod/blob/47cfee3f/<path>#L<n>`. The clone was read only. No install, build or binary was run.

**Licence.** Root `LICENSE:1` is Apache License 2.0 and `NOTICE:1-8` names n8n GmbH. That is an OSI licence, so nod's code may legally be reused with its licence text and notice kept. The two libraries that do the provider work, `@earendil-works/pi-ai` and `@earendil-works/pi-coding-agent` (`package.json:43-44`, both pinned `1.5.2`), are MIT (npm registry metadata, checked 16 November 2026). One file, `src/providers/local-speech/phonemizer-worker.ts`, is GPL-3.0 (`NOTICE:5-8`) and is speech, not models. Retinue is MIT (`LICENSE:1`), so licence compatibility is not the obstacle anywhere in this report. The obstacle is architecture: their code is TypeScript in a Node server around a TS agent library, and ours is a Rust daemon around someone else's CLI. Every idea below is therefore "learn from the design", never "lift the code".

## Summary

1. nod has no provider abstraction of its own: it wraps **Pi** (`@earendil-works/pi-ai` + `pi-coding-agent`) behind two ports of its own, `ModelBackend` and `Agent`.
2. A provider is a **registry row** (`apiKeyModelProviders`, `modelProviderInfo`) plus, for services whose catalogue API is odd, one function; models and limits come from Pi's bundled catalogue narrowed by the provider's own `/models`.
3. Keys and sign-ins are a typed credential union, sealed with AES-256-GCM in Postgres under a master key from Keychain, DPAPI or a 0600 file. **nod holds every provider credential.**
4. Streaming, tool calls, images and effort are normalised by Pi into one message stream, which nod maps to its own `AgentEvent`; effort is a seven-level ladder mapped per model.
5. Retinue can adopt several **shapes** (declared capability rows, a per-model failure ledger, capability and price cues in the picker) but not the model: we hold no keys and call no model API, and a direct-API path would need an AGENTS.md amendment.

## How nod does it

### The design, in three layers

1. **Capability ports** (`src/ports/`). Pure interfaces, no vendor types. The two that matter here: `ModelBackend` (`src/ports/models.ts:62-89`: `connectKey`, `signIn`, `catalog`, `authorize`, `exchange`, `refresh`) and `Agent` (`src/ports/agent.ts:399-402`, with `AgentSession` at `:385-398`).
2. **Vendor providers** (`src/providers/`). Only `src/providers/pi/` does models and the agent loop. Everything a provider needs differently (an OAuth flow, a Cloudflare account catalogue, a local Ollama probe) lives there, plus `src/providers/{keychain,filesystem,environment}/key-store.ts` for the master key.
3. **Product policy** (`src/server/`). `src/server/models.ts` (61 KB: connections, routes, failures, selection) and `src/server/model-store.ts` (Postgres rows). Their own `AGENTS.md:32` states the rule: "`src/server/composition.ts` is the only place that selects providers".

The layers are not decorative. It means Pi may be replaced without touching the UI, and a new provider is normally data, not a subsystem.

### Main files

| File | What it does |
|---|---|
| `src/shared/models.ts` | The registry. `apiKeyModelProviders` (`:1-41`, 39 ids), `modelProviders` (`:44`, those 39 plus `compatible`), `ModelMethod` (`:46`), `ModelEffort` (`:47`, 7 levels), `ModelChoice` (`:69-78`), `ModelSelection` (`:87-91`), `routeModel` (`:122-143`), `modelProviderNames` (`:185-227`), `modelProviderInfo` (`:229-373`: key URL, env var, `catalog: 'account' \| 'bundled'`) |
| `src/ports/models.ts` | `ModelCredential` union (`:10-25`), `ModelAccess` (`:48-60`), `ModelBackend` (`:62-89`) |
| `src/providers/pi/models.ts` | `createPiModelBackend` (`:200`): ChatGPT PKCE sign-in (`:446-631`), `catalog` (`:696-903`), per-provider catalogue parsers (`:376-444`), `createPiModelRuntime` (`:1014`) that turns a saved connection into a Pi `ModelRuntime` |
| `src/providers/pi/model-catalog.ts` | `nativeCatalog` (`:96-315`) — one branch per market catalogue API; `assistantModels` filter (`:80-82`) |
| `src/providers/pi/index.ts` | `createPiAgent` (`:415`): builds tools, the Pi session (`:1038-1050`) and maps Pi session events to nod's `AgentEvent` (`:1158-1307`) |
| `src/providers/pi/usage.ts` | `costEstimate` (`:11-30`), `piUsageMetrics` (`:67-85`), `createPiUsageTracker` (`:87-134`) |
| `src/providers/pi/context.ts` | One context/compaction policy for every model (`:1-33`) |
| `src/server/model-store.ts` | `model_connections` and `model_settings` tables; credentials written to the vault (`:42-169`) |
| `src/server/vault-app-grants.ts`, `vault-crypto.ts` | Sealed credential storage: AES-256-GCM, AAD binds `['nod-app-grant-v1', owner, id]` (`vault-crypto.ts:3-10`) |
| `src/server/composition.ts:328-342` | The one place that chooses the master-key store |
| `desktop/main/settings.mjs:380-392` | Desktop generates a 32-byte vault key once and protects it with Electron `safeStorage` (DPAPI on Windows) |
| `src/server/models.ts:465-526` | Per-model request-failure ledger and the backoff that pauses automatic work |

### The data flow of one turn

1. Settings › Models calls the server. `connectKey` stores the key, runs `catalog`, and returns `{credential, models}` (`src/providers/pi/models.ts:633-695`). An OAuth sign-in instead returns a URL, a state and a promise (`:446-631`).
2. `catalog()` builds a list of `ModelDefinition` (`src/ports/models.ts:43-46`): `choice` for the picker, `spec` for the request. It starts from Pi's bundled list, narrows it by the provider's own list, and stores the result as `jsonb` on the connection row (`src/server/model-store.ts:133-148`).
3. A turn starts. `routeModel` chooses a `ModelSelection` from task model, schedule model, lane, light model, then the owner's selection (`src/shared/models.ts:122-143`).
4. `createPiModelRuntime` registers that one model with a private Pi `ModelRuntime` whose `CredentialStore` reads the sealed credential at request time (`src/providers/pi/models.ts:1043-1105`). The credential is never put in a prompt, a workspace file or a log.
5. `createPiAgent` opens a Pi session with `thinkingLevel: access.effort` (`src/providers/pi/index.ts:1043`). Pi streams; nod's subscriber maps each Pi event to `text_delta`, `thinking`, `retry`, `message_start/end` and an `AgentEvent::ModelUsage` (`:1158-1307`).

### How a provider is added

Normally one registry entry, in three places in one file:

- add the id to `apiKeyModelProviders` (`src/shared/models.ts:1-41`);
- add a `modelProviderNames` line (`:185-227`);
- add a `modelProviderInfo` row (`:229-373`) with the key page, the env var name, and whether the model list comes from the account (`catalog: 'account'`) or only from the bundled catalogue (`'bundled'`).

Then Pi's own catalogue must know the models. That is a dependency bump, or a local override object such as `antLingModel` (`src/providers/pi/model-catalog.ts:46-74`). Two cases still need code:

- **A catalogue API that is not OpenAI-shaped:** a branch in `nativeCatalog` (Cloudflare paging, Google paging with `nextPageToken`, Mistral capability flags, Radius, then the generic `/models`) (`:153-315`).
- **A sign-in that is not a key:** a new `ModelCredential` variant and a branch (`oauth` for a ChatGPT plan, PKCE for OpenRouter, `endpoint` for an OpenAI-compatible server).

So "many providers" is a registry plus a thin adapter per odd service. The heavy lifting (request shapes, retries, streaming, tool calls) is Pi's, not nod's.

### Models and their limits

Three sources, in this order:

1. **Bundled** (`catalog: 'bundled'`): Pi's `getModels(provider)`, filtered to models that take text and are not specialists (`image`, `tts`, `embedding`, `rerank`, `computer-use`…) by `assistantModels` (`src/providers/pi/model-catalog.ts:80-82`, filter at `:19-20`). Context window, max tokens and prices come from the bundle.
2. **Account** (`catalog: 'account'`): the provider's `/models` returns ids, and nod **intersects** them with the bundled list, so the ids are live but the limits and prices stay the reviewed bundle values (`nativeCatalog:148-151`, `select(ids)`).
3. **Live and complete:** OpenRouter is the exception. Its list *becomes* the models, with `context_length`, `top_provider.max_completion_tokens`, `architecture.input_modalities` (images) and per-token `pricing` converted to USD per million (`src/providers/pi/models.ts:823-902`). Local servers are the other exception: `/models`, then Ollama `/api/show` or LM Studio `/api/v1/models` for context, thinking levels and vision (`:259-363`).

`definition()` then produces what the profile sees: `{id, name, efforts, images, audio?, contextWindow}` (`:245-258`), where `efforts` is `getSupportedThinkingLevels(model)`. Limits are clamped per session by `modelContext` (`src/providers/pi/context.ts:1-33`): the window shrinks for audio companions and OpenRouter routers, `maxTokens` is at most half (a quarter on small windows), and windows below 65,536 tokens switch nod into a "compact" mode (short instructions, one detailed tool at a time, early compaction).

### Keys and sign-ins

- **Typed, not strings.** `ModelCredential` is a union of `api_key`, `endpoint`, `bedrock`, `cloudflare`, `azure`, `oauth` (`src/ports/models.ts:10-25`). Each carries exactly what that provider needs (region, account id, gateway id, workspace id, deployment).
- **Sealed at rest.** The credential is JSON, encrypted with AES-256-GCM and bound by AAD to owner + connection id, and stored in `vault_app_grants` / read back per connection (`src/server/model-store.ts:56-66`, `:122-132`; `src/server/vault-crypto.ts:3-25`).
- **One master key, four homes.** `NOD_VAULT_KEY` from the environment (self-host), Electron `safeStorage` on desktop (`desktop/main/settings.mjs:380-392`), macOS Keychain through `/usr/bin/security` (`src/providers/keychain/key-store.ts:9`, `createKeychainKeyStore`), or a `0600` file under the data root on Linux, with ownership and mode checks before use (`src/providers/filesystem/key-store.ts:8`, `createFilesystemKeyStore`). `src/server/composition.ts:328-342` is the only place that picks.
- **Sign-ins.** ChatGPT: PKCE, a loopback callback on `127.0.0.1:1455`, and the ID token verified against OpenAI's JWKS with the issuer, audience, nonce and account checked (`src/providers/pi/models.ts:446-631`). OpenRouter: PKCE and a key exchange (`:904-944`). Refresh is silent, and a terminal refusal becomes "Reconnect" (`:945-975`).
- **Display.** The picker shows an account email, or the last four characters of a key (`keyEnding`, `src/shared/models.ts:165`). It never shows the key itself. The model never sees either.

### Streaming, tool calls, images, effort

- **One stream shape.** Pi emits text, thinking, tool calls and a stop reason. nod maps it to `AgentEvent` (`src/ports/agent.ts:333-347`): `text_delta`, `thinking {title}` (nod digests the model's thinking into short titles, `src/providers/pi/index.ts:1242-1253`), `retry`, `final`, `message_end {status, contextExhausted}`.
- **Tools are nod's, only executed by Pi.** Tools are declared once as `AgentTool` (`src/ports/agent.ts:303-314`) and adapted into Pi `ToolDefinition`s with an exposure mode (direct or deferred behind `nod_tool_search`) and a parallel/sequential mode (`src/providers/pi/index.ts:657-701`). So tool schemas, approval and output bounds are nod policy; transport and model-specific tool-call syntax are Pi's.
- **Images.** A model reads images when its catalogue entry includes `'image'` in `input` (`src/providers/pi/index.ts:1329`). One extra rule: if the selected model cannot read images, nod sends the images to another connected service's image-capable model and passes the description back (`docs/connect/models/`, "Images with a text model").
- **Effort.** Seven levels: `off, minimal, low, medium, high, xhigh, max` (`src/shared/models.ts:47`). Each model maps each level to a provider value, or to `null` when unsupported (`thinkingLevelMap`, e.g. `src/providers/pi/model-catalog.ts:57-65`). The picker offers only supported levels, and `lightestEffort` picks `off`/`minimal`/`low`/`medium` for background work (`src/shared/models.ts:65-67`).

### Usage and cost

- **One row per model request.** `createPiUsageTracker` turns each finished assistant message into one `model_usage` observation with a deterministic id (SHA-256 of provider, api, model, response id, then a UUID), so replaying stored history into a new session cannot double count (`src/providers/pi/usage.ts:32-65`, `:87-134`).
- **Buckets stay nullable.** `input`, `output`, `cacheRead`, `cacheWrite` and `total` are copied only when they are safe integers (`:67-85`, `:99-128`).
- **Cost is an estimate, or nothing.** It comes from Pi's own price table, marked `costSource: 'sdk_estimate'`. `costEstimate` rejects a price set whose parts do not add up to its total, and audio pricing that is unavailable, returning `null` rather than zero (`:11-30`).
- **A subscription is not a price.** For a ChatGPT-plan connection, nod rewrites the usage so `costUsd` is `null`, `estimateUsd` keeps the estimate and `pricing` becomes `'unavailable'` (`src/providers/pi/index.ts:443-469`). That is the same honesty rule we have: a plan is not a per-turn charge.
- **Answering "what would it cost elsewhere".** `repriceUsd` re-prices a lane's last-30-days usage at another model's rates (`src/shared/models.ts:145-155`), which is what powers the per-lane cost comparison in Settings.

## How Retinue does it now

### The shape: adapters around CLIs, not a model client

Retinue never calls a model API. The daemon runs a coding-agent CLI and speaks its protocol. `AgentAdapter` (`crates/retinue-agent-core/src/lib.rs:1466-1530`) is the port: `probe`, `new_session`, `resume_session`, `prompt`, `steer`, `cancel`, `reply_permission`, `close_session`, plus optional `model_catalog`, `read_account_quota` and `model_switch`. Three adapters implement it: Claude Code (stream JSON), Codex (app server) and ACP (OpenCode and the table rows).

Events are ours, not Pi's: `AgentEvent` (`crates/retinue-agent-core/src/lib.rs:1308-1438`) covers deltas, thinking, tool rows, permission cards, questions, plans, images, usage, recovery hints and context changes.

The only direct provider HTTP calls in the tree are the two owner-approved exceptions: usage bars (`crates/retinue-daemon/src/quota_sources.rs:12-13`) and opt-in cloud dictation (`crates/retinue-speech/src/cloud.rs`). `AGENTS.md:44` and `:153` describe exactly that, and `:215` keeps the general rule: "No copied provider authentication … fabricated success/usage, or unknown prices presented as zero."

### The provider table

`crates/retinue-runtime/src/providers.rs:34-84` is one row per ACP provider: `{provider, display_name, command, args, sign_in_command, install_hint, probe_token}`. OpenCode is deliberately not a row, because it has its own credential read and flags (`:1-15`). The three main providers are always on (`:86-89`); every other provider is optional and off until the owner turns it on (`crates/retinue-daemon/src/provider_gate.rs:1-17`). Adding a provider is a row, and discovery (`crates/retinue-runtime/src/lib.rs:260-324`) finds it.

Two things are still hard-coded beside the row: the capability set (`:311-322`: `resume | usage => matches!(id, "claude" | "codex")`) and the protocol family (`:265`). A new ACP provider therefore needs an edit in the table *and* a read of that match, which is the one place nod's data-first habit is ahead of us.

### Catalogues

`crates/retinue-daemon/src/catalog.rs:1-40` caches a CLI probe (Claude 2-8 s, Codex 5 s, OpenCode up to 20 s): fresh 15 minutes, stale 7 days, failures remembered 30 seconds, one probe joined per key, and a `CachedOnly` mode that never blocks a turn. `ModelInfo` (`crates/retinue-agent-core/src/lib.rs:1120-1150`) carries `id`, `displayName`, `supportedThinking`, `defaultThinking`, `serviceTiers`, `variants`, `contextWindow`, `availability`, `releaseDate` and a display `group`. It carries no price and no image flag.

`model.custom.*` (`crates/retinue-daemon/src/custom_models.rs:1-17`) is the owner's escape hatch for a model id the CLI accepts but does not list; the turn passes the name through and a refusal fails that turn honestly.

### Effort and settings evidence

Effort is the provider's own vocabulary, reported by the CLI, shown with per-provider labels (`apps/desktop/src/ui/effortLabels.ts:1-18`). After a turn, `ExecApplied` reports per-setting `SettingOutcome`s with an `EvidenceKind` (`crates/retinue-agent-core/src/lib.rs:1199-1212`: `none`, `request_shape`, `provider_echo`, `usage_effect`, `successful_turn`), so an effort the CLI silently dropped is visible rather than assumed.

A model change is planned before it happens: `session.model.plan` (`crates/retinue-daemon/src/main.rs:1630`) and `modelChangeNeedsWarning` (`apps/desktop/src/ui/SessionExecutionControls.tsx:33-40`), because `ModelSwitch::NewSession` is the default for any adapter that has not proved it can keep the session (`crates/retinue-agent-core/src/lib.rs:1453-1471`).

### Accounts, secrets, and the absent key

An account is a config folder the CLI is pointed at by environment (`crates/retinue-daemon/src/accounts.rs:44-57`: `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `XDG_CONFIG_HOME`/`XDG_DATA_HOME`). Who is signed in is read from the CLI's own output, never from a token (`accounts.rs:1-5`).

Secrets are for bot environment variables and one-use requests, not for providers: Credential Manager under `Retinue/<name>`, read only at spawn, no RPC returns a value (`crates/retinue-daemon/src/secrets.rs:1-19`). There is no field anywhere for an OpenAI key, by design.

### Usage and cost

`UsageReport` keeps every bucket nullable plus a status string (`crates/retinue-agent-core/src/lib.rs:1161-1185`). Pricing runs through `PriceRule`s keyed by provider and canonical model, with aliases, tiers (threshold, weekday, window), effective dates and a source URL (`crates/retinue-usage/src/lib.rs:27-77`), and `Valuation` reports one of five bases: `provider_reported_actual`, `api_rate_estimate`, `subscription_fixed`, `local_free`, `unknown` (`:18-26`). An unknown price stays unknown. There is no live price API, so our prices are a shipped, reviewed file rather than a provider's current table.

### What this means for "more providers"

For Retinue, "more providers" has two different meanings, and they cost very different amounts:

1. **More coding-agent CLIs** (a new ACP row). Cheap and already in the rules: one row, a probe token, a sign-in command, an install hint, and a capability claim that must be proved.
2. **More model APIs** (OpenAI, Anthropic, Gemini direct, OpenRouter). This is what nod offers. It cannot be done inside our current rules without an amendment, because the daemon would have to hold a provider credential and call a model endpoint. `AGENTS.md:215` and the 30 October usage-bar exception (`AGENTS.md:44`) both say the exception is for usage bars only.

## What they do better

1. **Breadth reads as a checklist, not a project.** Thirty-nine named providers plus an OpenAI-compatible entry in one picker, each with its key page, its env var and whether its catalogue is per-account or bundled (`src/shared/models.ts:229-373`). The owner can add a connection while the app is running.
2. **A per-model failure ledger with a pause that only affects unattended work.** Failures are stored per connection and model, keyed by a credential hash, with a backoff of `min(2 min × 3^(n-1), 6 h)` (`src/server/models.ts:291-301`, `:465-526`). Automatic work is refused with the provider's own reason; a turn the owner starts still runs; a success clears the model and un-pauses its siblings (`:495-500`). Their docs are explicit that a model is never switched on the owner's behalf.
3. **One honest rule for "we know this id, we do not know its limits".** Live ids intersected with bundled metadata (`model-catalog.ts:148-151`) is a clean way to say "this exists" without inventing a context window.
4. **Capability and price cues reach the picker.** Image, audio and thinking icons, plus per-million prices for OpenRouter (`docs/connect/models/`). Our picker shows thinking levels and service tiers, and neither a price nor an image flag.
5. **One context policy for every model,** including a deliberate small-window mode below 64 k (`src/providers/pi/context.ts:1-33`). We adapt context per adapter and per CLI, so the same model can behave differently in two conversations.
6. **Model choice per kind of work, with the price shown.** Task, schedule and background each resolve their own model (`src/shared/models.ts:122-143`), and Settings shows what each lane cost, repriced at the alternative's rates (`:145-155`). We have per-bot and per-project models, but no "automations use the cheap model" choice.

## What we do better

1. **We hold no provider credential at all.** nod must seal keys in Postgres, pick a master key per platform, and check file permissions. Our threat model has no "we hold your OpenAI key" row, and no key path into a sandbox.
2. **We prove settings were applied instead of trusting the library.** `ExecApplied` with `EvidenceKind` (`retinue-agent-core/src/lib.rs:1199-1212`) and the requirement that `model_switch` be justified by file:line (`:1468-1471`) are checks nod has no equivalent of.
3. **A model change is an event the owner sees.** Warning, plan, lock and recap handoff. nod re-routes per turn and per lane silently, which is right for their product and wrong for a conversation whose context would be lost.
4. **Cost has five honest bases, not one estimate flag.** `CostBasis` separates a provider-reported actual from an API-rate estimate, a subscription and a local model (`retinue-usage/src/lib.rs:18-26`). `provider_reported_actual` does not exist in nod's ledger at all.
5. **A new provider carries its own evidence of existence.** A `probe_token` in the `--help` output (`providers.rs:30-31`, checked at `retinue-runtime/src/lib.rs:279-292`) is a stronger claim than "Pi's bundle mentions it".
6. **No silent model choice.** A conversation with no model says so, a change that loses context asks, and `model.custom` refuses a name the catalogue already lists (`custom_models.rs:1-17`).

## Ideas we could adopt

Effort is small (S, a day or two), medium (M, a wave item) or large (L, needs a plan). "Rules" names the AGENTS.md rules an implementation would touch.

### 1. Put the capability claims inside the ACP provider row — S

**What.** Add `capabilities { resume, usage, permissions, images, context }` and `catalog` to `AcpProvider` (`crates/retinue-runtime/src/providers.rs:17-32`), defaulting to `false`, and have `discover_one` read the row instead of the `matches!` at `crates/retinue-runtime/src/lib.rs:311-322`. Adding a provider stays one row and stops needing a second edit. The `images` field would also replace the hard-coded provider allowlist at `apps/desktop/src/ui/App.tsx:2848`, which today gives image attachments to the three main CLIs only, so a new ACP provider gets none even when it can read them.

**Why.** It is nod's one genuinely better structural habit: a registry row states everything about a provider, including what it can be trusted to answer.

**Risk.** Low. The only danger is a row claiming a capability its CLI lacks, which the existing probe and evidence checks would then report as a failure. Every field defaults to `false`, so an unedited row claims nothing.

**Rules.** The honesty rule (`AGENTS.md:215`). `docs/ipc-contract.md:621` is already stale (it still names only Gemini and Qwen), so this is the moment to fix it. No tier change: `provider.list` stays `never`.

### 2. A per-model failure ledger that pauses only unattended work — M

**What.** Persist `(provider, model, credential-ish tag) → {failures, retryAt, reason}` and refuse automatic turns (automations, scheduled tasks, the CEO's own wakes) to a paused model with the provider's reason, while an owner turn always runs. Clear it on the next success. Surface it in Settings › Coding agents and in the composer status row.

**Why.** We already have usage-limit notices and a watchdog, but nothing remembers "this model failed an hour ago" across restarts. Their ladder (`src/server/models.ts:296-297`) is a sensible, cheap shape.

**Risk.** Medium. It can look like Retinue is deciding for the owner. It must never switch models, never hide a model from an attended turn, and never be described as a budget (budgets are retired). The reason string comes from the provider and must be sanitised the way our other provider text is.

**Rules.** "The daemon owns decisions" and "unknown as unknown" (skill golden rules 2 and 3); `AGENTS.md:215`; budgets retired (`AGENTS.md` 12 October amendment); the Remote tiers (`apps/desktop/src/remote/tiers.json`) — carry the state on the existing `runtime.models` / `provider.list` payloads so no new method and no new event is needed, since `runtime.*` and `provider.*` are `never` on a phone today.

### 3. Capability and price cues in the model menu — S

**What.** In the picker (`apps/desktop/src/ui/SessionExecutionControls.tsx`, `modelPresentation.ts`), show which models read images, which take a service tier, and the price per million tokens from our own `PriceRule` data, with "price unknown" where there is no rule.

**Why.** The owner currently has to know that a given OpenCode model takes images, or what it costs. nod shows both, and it is the cheapest part of their breadth advantage to copy.

**Risk.** Low to medium. A price must come from the shipped rule file and carry its source; an unknown price must read as unknown, never as 0. Images must not be claimed per model unless the CLI reports it; today our attachment rule is per runtime (`imagesSupported`, `ComposerControls.tsx:200`), and that is honest, so this idea should start with prices and tiers only.

**Rules.** `AGENTS.md:215` ("unknown prices presented as zero" is forbidden); the design language (`references/design-language.md`) for icons and rows; the Remote tiers: `runtime.models` is `never`, so phones keep the plain list.

### 4. A `lightest_effort` helper and a per-lane model choice — M to L

**What.** A helper over `ModelInfo.supported_thinking` that returns the cheapest offered level, mirroring `lightestEffort` (`src/shared/models.ts:65-67`), so automations, titles, the CEO and sub-bot wakes can ask for "the cheapest effort this model offers" without per-provider tables. Then, if the owner wants it, a "Work on its own uses" choice (same model, or a nominated cheaper model) stored per project or globally.

**Why.** It converts an existing judgement call ("run scheduled tasks on something cheap") from a manual per-bot setting into a declared policy, and it is the one nod feature with a real cost outcome.

**Risk.** Medium. A different model per lane means a new provider session per scheduled run, which is fine because each run already has its own conversation; but it must be visible, priced, and reversible. Do not present it as spend control.

**Rules.** Budgets retired; the daemon owns sessions; `provider_gate` (an optional provider chosen for a lane must still be on); `settings.set` is desktop-only, so any new setting inherits the Remote rule; the design language for the Settings row.

### 5. "Your own model server", wired into a CLI rather than into Retinue — M

**What.** A Settings row that points a chosen CLI at an OpenAI-compatible server the owner runs (Ollama, LM Studio, vLLM, llama.cpp): write the CLI's own provider configuration or the environment variable, then verify by starting the CLI and reading back `runtime.models`. The key, if any, goes through `secrets.rs` as an environment reference, never into the database.

**Why.** It answers "can I use my local model" without Retinue becoming a model client. OpenCode and Qwen Code already accept provider configuration; the CLI holds the connection, signs with its own rules, and prints its own errors. This is the permitted shape of nod's `compatible` provider (`src/providers/pi/models.ts:259-363`).

**Risk.** Medium. Each CLI's config format drifts, so each needs a test. Writing files into a CLI's config folder must be bounded to that folder and reversible. Do not proxy traffic and do not store the server's key in our database.

**Rules.** Golden rule 3 (`AGENTS.md:215`) is preserved, because the CLI holds the key; the loopback rules are untouched, because the daemon never listens and never calls the server; `provider_gate` for optional providers; the 14 November browser and computer-control amendment is not touched (no `owner_*` or `computer_*` method).

### 6. Adopt nothing else from their provider layer — no effort

**What.** Do not add a direct model-API client, a key store for provider keys, or per-request cost reservations.

**Why.** Each would need an amendment to `AGENTS.md:215` and the threat model, duplicate the CLIs' own sign-in and billing, and put a credential store inside the daemon — the exact thing our design avoids. nod needs a key because it has no CLI to borrow; we have three.

## Open questions for the owner

1. **What does "more providers" mean here** — more coding-agent CLIs, or models reached by API? If it is the second, the first step is an `AGENTS.md` amendment plus a threat-model row, not a lane.
2. **Should an unattended run be blocked by a model that just failed?** Idea 2 is cheap but it is a policy change: today we run the turn and let it fail.
3. **Do you want a per-lane model choice** (automations and background work on a cheaper model), or is the per-bot model enough?
4. **May Retinue write a CLI's own provider config** for a local server (idea 5), or should the owner always do that in the CLI itself?
5. **Do you want prices in the model menu,** given our prices come from a shipped rule file rather than the provider's live list?
6. **Should the Antigravity row stay?** It is in the table but unverified on Windows (`providers.rs:64-83`). A capability field (idea 1) would make that visible in the UI rather than in a comment.
