---
title: Install Retinue
description: Download Retinue, check the installer, and let it find the coding agents already on your PC.
section: Getting started
order: 1
---

# Install Retinue

Retinue is a Windows app. You install it like any other app, and it finds the coding agents you already use. You do not need to create an account with Retinue.

## Before you start

- Install at least one coding agent and sign in to it once. Retinue works with Claude Code, Codex and OpenCode. The others (Qwen Code, Cursor, Grok Build and Antigravity) are off until you switch them on.

## Steps

1. Open the [Retinue releases page](https://github.com/sico-vibes/retinue/releases) and download `Retinue_<version>_x64-setup.exe` for the latest version.
2. Download `SHA256SUMS.txt` from the same release. Check that the installer matches the checksum listed there. This proves the file is the one the release published.
3. Run the installer and follow the steps.
4. If Windows warns you, see the note below. The installer is not code-signed yet.
5. Open Retinue. It looks for your coding agents and lists the ones it finds. If nothing appears, choose **Scan for agents**.
6. Pick the coding agents you want to use. You can change this later under Settings, Coding agents, Providers.

## If Windows stops the installer

Retinue's installer is not code-signed yet, so Windows may stop it. This is expected for early betas.

- **Microsoft Defender** may report the file as `Program:Win32/Contebrew.A!ml`. This is a false positive. To keep the file, open Windows Security, go to Protection history, select the entry and choose **Allow on device**.
- **SmartScreen** may say Windows protected your PC. Choose **More info**, then **Run anyway**.
- If your PC has **Smart App Control** turned on, Windows blocks unsigned installers and gives no way to run them. Retinue can be installed there once releases are signed.

## Updates

Retinue 0.1.0-beta.32 and later update themselves. When an update is ready, the app shows it. You do not need to download the installer again.

## Good to know

- Retinue keeps its data on your PC. Back it up from Settings, System, Backup.
- If a coding agent is not listed, it is probably not installed or not on your PC's path. Install it, sign in, then choose **Scan for agents** again.
