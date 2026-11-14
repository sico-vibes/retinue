---
title: Linked machines
description: Pair a second computer with Retinue, talk to its employees from here, and choose what this machine may do there.
section: Anywhere
order: 3
---

# Linked machines

A linked machine is another computer running Retinue that you have paired with this one. Its employees appear under that machine in the sidebar, so you can talk to them from here, while they run on the other computer with its own coding agents, files and sign-ins.

![The linked machines page](/docs/docs-machines-dark.webp)

## Before you start

- Retinue must be running on the other machine.
- Pair the two machines once, in Settings, then Connections, then Remote access, then Machines. Tailscale is the simplest way to reach the other machine.

## Steps: talk to an employee on another machine

1. Open the linked machine in the sidebar, then choose the employee.
2. Choose **New conversation** to start one on that machine.
3. Type your message and choose **Send**. The other machine runs the turn.
4. If the employee is already working, choose **Send now** to add your message to the running turn, or **Stop** to end it.

## What each level allows

The level is set on the machine you are working on, and it decides what this machine may do there:

- **View** shows the conversations, read-only.
- **Chat** lets you talk to the employees.
- **Operator** also lets you take conversations over.

## Good to know

- A conversation runs on one machine at a time. Taking a conversation over moves it to the machine you choose.
- A message is refused with a plain reason when the level does not allow it, or when the other machine cannot be reached.
- A Linux server cannot run Retinue yet. Use [A cloud computer for your employees](/docs/a-cloud-computer-for-your-employees/) for work on a server.
