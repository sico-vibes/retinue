---
title: Your phone and the web version
description: Reach your employees from a phone on your own network, using Remote access. Set it up once, and approve each device yourself.
section: Anywhere
order: 2
---

# Your phone and the web version

Remote access lets a phone you own reach the employees on your PC. It works in a web page on the phone, not in a separate app. Remote is in beta. It is not turned on by default.

## What you need

- Retinue running on your PC, with Remote access turned on.
- The phone on the same private network as your PC. Tailscale is the simplest way to do this, because it makes the link private without opening your PC to the internet.

## Steps: pair a phone

1. Open Settings, then Connections, then Remote access, then Remote.
2. Turn Remote on.
3. On your phone, open the address Retinue shows and enter the one-time code. The code is valid for five minutes, and it allows five tries.
4. On your PC, a request appears asking you to approve the phone. Choose the level it may use:
   - **View** can see conversations, but cannot send anything.
   - **Chat** can send messages to employees.
   - **Operator** can also take conversations over from another machine.
5. Approve the phone.

## Good to know

- Each phone is approved separately. A phone can only do what its level allows.
- Message text is not written to logs or activity lists.
- Remote is still in beta. Test it with a real phone before you rely on it.
