---
title: Coding agents and providers
description: Which coding agents Retinue works with, which are on by default, and how to switch the others on.
section: Connect
order: 1
---

# Coding agents and providers

Retinue does not have its own AI model. Each employee runs on a coding agent you already have, such as Claude Code, Codex or OpenCode. The coding agent does the work with the model and the sign-in you chose for it.

![The providers settings page](/docs/docs-providers-dark.webp)

## The coding agents

- **Always on:** Claude Code, Codex and OpenCode.
- **Off until you turn them on:** Qwen Code, Cursor, Grok Build and Antigravity.

## Steps: turn a coding agent on

1. Open Settings, then Coding agents, then Providers.
2. Find the coding agent you want.
3. Turn it on. Retinue checks that it is installed and signed in.
4. If it is not installed, install it from its own website, sign in once, then choose **Scan for agents**.

## Steering

For Claude Code and OpenCode you can choose whether a message can be sent into a running turn. Turn steering on or off on the Providers page. See [Messages, the queue and steering](/docs/messages-the-queue-and-steering/).

## Good to know

- A coding agent's sign-in stays with that agent. Retinue does not copy it.
- New conversations use the agent you chose for the employee. Existing conversations keep theirs.
- If a coding agent's sign-in has expired, Retinue flags it and offers to fix it.
