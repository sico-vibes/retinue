# Privacy

Retinue runs on your computer. It has no account, no analytics and no telemetry, and it sends nothing to its author.

Your conversations, projects, settings and usage figures stay in a local database (`%LOCALAPPDATA%\Retinue`). The coding agents you use (Claude Code, Codex, OpenCode) run as their own programs with their own sign-ins. Each one talks to its own provider under that provider's terms, as it would without Retinue.

## When Retinue itself goes online

Retinue goes online only for the following, each to the service named.

| What | Where it connects | When |
| --- | --- | --- |
| Checking for a Retinue update | GitHub (`github.com`), the release feed | On start-up, and when you choose **Check for updates**. You can turn the automatic check off in Settings, Updates. |
| Checking for a coding-agent update | The npm registry (`registry.npmjs.org`): the version number of each agent you use | When Retinue shows the update notice for your agents. It installs an update only when you choose to. |
| Downloading the on-device speech model | The model's download address shown in Settings | Only when you choose to download it. Dictation then runs on your computer. |
| Usage bars | Each provider's own usage endpoint (Anthropic for Claude, OpenCode) | Off by default. Only if you turn on reading the agents' sign-ins for usage. The sign-in is used for that single request and is never stored or logged. |
| Cloud dictation | OpenAI's transcription endpoint (`chatgpt.com`), with your Codex sign-in | Off by default. Only if you turn it on. Audio and transcripts are not stored or logged. |
| Remote | Your own devices, over your Tailscale network or Retinue's own HTTPS listener | Off by default. Only devices you approve on the desktop can connect. |
| Machine links | Another Retinue computer that you paired | Only after you pair it and approve the link. |

The installer may download Microsoft's WebView2 runtime if Windows doesn't already have it.

## Third-party components

Retinue is built from open-source components. These are listed with their licences in [docs/legal/third-party-notices.md](docs/legal/third-party-notices.md). None of them sends data anywhere on its own.
