# Wave 3, MCP OAuth lane: contract for the screens

> **Status:** living. Written before any code (waves 1 and 2 lesson). The Director builds the
> Connect button and the sign-in status from this file; every shape here is stable unless a
> section says "changed".
> **Lane:** Ada, MCP OAuth for connectors. Source: `docs/research/n8n-nod/03-connectors.md`
> idea 2, plan `docs/plans/nod-adoption-plan.md` wave 3 and section D.
> **Code:** `crates/retinue-daemon/src/connectors_oauth.rs` (new), small hooks in
> `crates/retinue-daemon/src/connectors.rs` and `main.rs`, rows in
> `apps/desktop/src/remote/tiers.json`, `docs/remote/THREAT_MODEL.md` and
> `docs/ipc-contract.md`.

## What changes for the UI

A connector whose `auth` is `oauth` is added like any other (no token), then signed in with a
browser. Three RPCs, one event, one optional parameter on `connector.add`, and one behaviour
change on `connector.update` and `connector.remove`:

1. `connector.add` accepts `auth: 'oauth'` (optional; existing calls are unchanged).
2. `connector.oauth.start`, `.status`, `.cancel` — the sign-in's life.
3. Event `connector.oauth.changed`.
4. Removing a signed-in connector revokes its token at the service first (best effort).
5. The **Connector DTO does not change**. The screen reads the sign-in state with
   `connector.oauth.status { id }` for each row whose `auth` is `'oauth'` (so the badge is
   correct after a restart too).

Nothing else changes: `connector.list`, `.tools`, `.tool.rule`, `.test`, `connector_search`
and `connector_call` keep their shapes.

## Types

```ts
type ConnectorSignIn = 'idle' | 'pending' | 'authorized' | 'error'

type ConnectorOAuthStatus = {
  id: string                 // the connector id
  auth: 'none' | 'token' | 'oauth'   // the row's auth kind
  status: ConnectorSignIn
  detail: string | null      // plain sentence for `error`/`pending`; null otherwise
  expiresAt: string | null   // ISO instant, only while `pending` (the five-minute deadline)
}
```

- `idle`: the connector has no saved sign-in and no flow is running.
- `pending`: the browser flow is waiting for the owner (up to five minutes).
- `authorized`: tokens are in Credential Manager. A token is refreshed automatically when it
  is within 60 seconds of expiry, so `authorized` does not mean "unexpired" — `connector.test
  { id }` is the honest check.
- `error`: the last attempt failed; `detail` says why in one sentence. Starting again clears
  it.

## RPCs

### `connector.add` (extended, optional field)

```
connector.add { name: string, url: string, token?: string, auth?: 'none' | 'token' | 'oauth' }
  -> { connector: Connector }
```

- `auth` is optional. When it is absent the rule is unchanged: a `token` makes it `"token"`,
  no token makes it `"none"`. `auth: "token"` without a token is `invalid_argument`.
- `auth: 'oauth'` refuses a `token` in the same call (`invalid_argument`) and creates the row
  with `auth: "oauth"`, `status: "unchecked"`, `detail: "Sign in to use this connector."`
  and no network check (there is nothing to send yet).
- Any other value is `invalid_argument`.

### `connector.oauth.start`

```
connector.oauth.start { id: string }
  -> { status: 'pending', authorizationUrl: string, expiresAt: string }
```

- Runs discovery, registers the client when the server offers dynamic registration, binds a
  loopback callback on `127.0.0.1` (an ephemeral port), builds the consent URL, **opens the
  owner's default browser at it**, and returns.
- `authorizationUrl` is the exact page that was opened: show it behind "Open the sign-in page
  again" if the owner closed the tab. It holds the random `state` and the PKCE challenge, never
  a token or a code. Do not render it as text anywhere it could be logged.
- `expiresAt` is five minutes after the call. After that the listener is closed and the status
  becomes `error` with "The sign-in was not completed in time. Try again."
- Errors (all are a `DispatchError` with a code):
  - `not_found`: no such connector.
  - `conflict`: a flow is already pending for it ("A sign-in is already waiting for this
    connector."), or the connector has a saved token ("This connector uses a saved token.
    Clear the token first.").
  - `unavailable`: the service's discovery or registration could not be read, or the browser
    could not be opened; the message names which step failed, never a URL with a query.
  - `invalid_argument`: the server offers no dynamic client registration ("This server does not
    offer automatic registration, so Retinue cannot sign in to it.").

### `connector.oauth.status`

```
connector.oauth.status { id: string } -> ConnectorOAuthStatus
```

- Safe to poll once when the page opens and on every `connector.oauth.changed`.
- Reading it never touches the network and never returns a token.

### `connector.oauth.cancel`

```
connector.oauth.cancel { id: string } -> { cancelled: boolean }
```

- `true` when a pending flow was stopped and its listener closed; `false` when nothing was
  waiting. The status becomes `idle`.

### `connector.update` (behaviour change, same shape)

```
connector.update { id, name?, token?, enabled?, unattended? } -> { connector }
```

- Unchanged for `token` connectors.
- `token: ""` on an `oauth` connector now also revokes the saved sign-in (best effort) and
  deletes it, and the row's `auth` becomes `"none"`. Use it for "Disconnect".
- Setting `token: "<value>"` on an `oauth` connector replaces the sign-in with the pasted token
  (`auth` becomes `"token"`).

### `connector.remove` (behaviour change, same shape)

```
connector.remove { id, confirm: true } -> { removed: boolean }
```

- For an `oauth` connector the daemon sends a revoke request to the service's revocation
  endpoint (RFC 7009) before deleting every credential entry. A failed revoke does not stop
  the removal; the reply shape is unchanged.

## Event

```
connector.oauth.changed -> ConnectorOAuthStatus
```

Emitted on `pending` (start), `authorized` (the callback succeeded), `error` (callback failed,
exchange failed, timeout) and `idle` (cancel, or a cleared sign-in). `drop` in `tiers.json`:
it never reaches a paired device.

`connector.changed` keeps firing for checks and edits, as before.

## Remote

| Method | Class |
|---|---|
| `connector.oauth.start` | `never` |
| `connector.oauth.status` | `never` |
| `connector.oauth.cancel` | `never` |

Event `connector.oauth.changed` is `drop`. Reason: the consent URL is a credential-granting
page, and the flow opens the owner's browser on the host; only the desktop can start it.

## How the sign-in works (for the copy, not for the screen)

1. **Discovery.** `initialize` without a token: a `401` whose `WWW-Authenticate` header names
   `resource_metadata` is read first; otherwise the RFC 9728 well-known addresses are tried
   (`/.well-known/oauth-protected-resource` and its path form). That document names the
   authorization server, then RFC 8414 metadata (`/.well-known/oauth-authorization-server`,
   then `/.well-known/openid-configuration`).
2. **Registration.** RFC 7591 dynamic client registration when the server offers it, with
   `token_endpoint_auth_method: "none"` (or what the server returns).
3. **PKCE.** `S256`, a 43-character verifier, a random 43-character `state`, and the RFC 8707
   `resource` parameter set to the connector's own address.
4. **Callback.** `http://127.0.0.1:<ephemeral>/callback` only. One use, five minutes, the
   `state` compared, then the listener is closed. A wrong or missing state answers `400` and
   the flow keeps waiting (it does not become an authorization).
5. **Tokens.** Exchanged at the token endpoint, stored in Windows Credential Manager and
   refreshed before expiry. Nothing logs the code, a token, or a URL that carries a query.

Owner-facing copy that matches the behaviour: **"Connect"** starts it, **"Waiting for your
browser"** while `pending`, **"Connected"** when `authorized`, **"Reconnect"** after `error`,
and **"Disconnect"** clears it (through `connector.update { token: "" }`).

## Storage (so nobody has to guess)

- One JSON bundle per signed-in connector at `Retinue/connector/<id>` in Windows Credential
  Manager: access token, refresh token, type, scope, expiry, client id/secret, endpoint
  addresses. Over the 2,048-byte credential cap it is split into `connector/<id>`,
  `connector/<id>.1`, `connector/<id>.2` … and joined on read (at most 8 parts, 16 KiB).
- A plain `token` connector still stores its token at `connector/<id>` as before.
- Nothing about the sign-in is in SQLite, an event, a reply, a log or a backup. `secret.list`
  does not show these entries.

## Running log

- **17 Nov, start.** Shapes above written first.
- **17 Nov, discovery and the flow.** `crates/retinue-daemon/src/connectors_oauth.rs`: RFC 9728
  protected-resource metadata (the `WWW-Authenticate` hint first, then the well-known address
  and its path form), RFC 8414 server metadata (then OpenID configuration), RFC 7591 dynamic
  registration (`token_endpoint_auth_method` from the answer, `none` by default), PKCE S256
  with a random state and the RFC 8707 `resource`, and the consent URL opened with the
  system's own handler.
- **17 Nov, the listener.** Bound to `127.0.0.1:0`, one flow per connector, one use, 300 s,
  the `state` compared; a wrong state answers 400 and the flow keeps waiting; the browser is
  answered before the code is exchanged; cancel aborts the task and waits for it, so the
  listener is closed when the reply says it is.
- **17 Nov, the credential store.** One JSON bundle at `Retinue/connector/<id>`, split into
  `connector/<id>.1` … over the 2,048-byte cap and joined on read (8 parts = 16 KiB bound, a
  larger bundle is refused rather than truncated). Both cases are tested.
- **17 Nov, refresh and revoke.** A refresh within 60 s of expiry behind one gate, re-reading
  the bundle after the lock; a failed refresh marks the sign-in `error` with "connect again";
  removal (and clearing the token) revokes at the service first, then deletes every part.
- **17 Nov, wiring.** `connector.add` takes `auth`; `connectors.rs` uses the OAuth access token
  for checks and calls; the three RPCs have their own `main.rs` arm; `tiers.json` has the three
  `never` rows and the `drop` event. The catalogue's OAuth rows now say "Connect with your …
  account" instead of "not supported yet".
- **17 Nov, tests run.** `connectors_oauth_tests.rs` (11 tests including the two wave-1 quota
  tests caught by the `oauth` filter) pass, and the eight wave-1 connector tests pass. Each of
  the three RPCs is called through `dispatch`, including the failure paths.
- **17 Nov, gates.** `cargo clippy -p retinue-daemon --tests --no-deps -- -D warnings` is clean;
  `node scripts/check-architecture.mjs` and `node scripts/check-format.mjs` pass (the new module
  has its allowlist row); the full daemon suite ran with the test binary copied to `C:\tmp`, from
  `crates/retinue-daemon` and with `TEMP=C:\tmp`: **1,233 passed, 0 failed**.
- **No shape changed** after the contract above was written.
- **Where the pieces are.** `connectors_oauth.rs`: the credential parts (`read_value` 114,
  `write_value` 128), discovery 377, the browser opener 506, the listener 614, registration 691,
  the code exchange 779, the access token and refresh 813/849, revoke 881, the RPCs 910, the flow
  941. `connectors.rs`: the bearer selector 642, `connector.add`'s `auth` 732, the update clear
  767, the remove revoke 793, the check's sign-in branch 586. `main.rs`: the module at 123 and the
  dispatch arm at 1209 (three methods).

