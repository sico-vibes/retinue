---
title: A cloud computer for your employees
description: Link a computer on your own server to Retinue, let employees work on it, and watch or take over its screen.
section: Anywhere
order: 4
---

# A cloud computer for your employees

You can link a computer on your own server, running Harbor, so your employees can work there. They clone repositories, build and run projects, and start long jobs in the background. You can see the computer's state and wake or sleep it from Retinue.

![The computer settings page](/docs/docs-computer-dark.webp)

## Steps: link a computer

1. Open Settings, then Connections, then Integrations, then Computer.
2. Enter the computer's address and its token. A Harbor set up from Retinue's setup prompt gives you both.
3. Check that the computer's state is shown. You can wake or sleep it from this page.
4. Turn on **Let employees use this computer**. New conversations then get the tools to work on it.

If you do not have a server yet, the page gives you a prompt to hand to a coding agent on your server. That agent sets the computer up.

## Let an employee work on the computer

1. Open the employee's settings.
2. Turn on **Works on the computer**. The switch appears once a computer is shared with employees.
3. Start a conversation. The computer wakes when the conversation starts, and Retinue never puts it to sleep for you.

Commands still wait for your approval when the employee is set to ask.

## Watch or take over the screen

Open the Computer card in a conversation's side panel, or **Open the screen** in Settings, then Integrations, then Computer.

- The screen opens in **Watching**. Employees use the computer, and nothing you do reaches it.
- Choose **Take control** to use the mouse and keyboard yourself.
- Choose **Watching** to hand the computer back.

## Good to know

- If the screen cannot be shown, Retinue says why, for example because the computer is asleep.
- Your server must accept the computer token on its screen page. A Harbor set up from Retinue's prompt does this.
- A Linux server cannot run Retinue yet. Use the computer feature for work on it for now.
