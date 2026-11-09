export const faq: { question: string; answer: string }[] = [
  { question: 'Which coding agents does it work with?', answer: 'Claude Code, Codex and OpenCode. Retinue drives the ones you’ve installed and signed in to, and each employee can run on any of them.' },
  { question: 'Does Retinue copy my sign-ins?', answer: 'No. Your employees run on the coding agents you’ve already signed in to. If you turn on usage bars or cloud dictation, Retinue reads the stored sign-in for that one request and sends it only to that provider. Both are off by default, and it’s never saved or logged.' },
  { question: 'Does my code leave my computer?', answer: 'Retinue itself runs on your computer. Your coding agents talk to their own providers, just as they do when you use them directly.' },
  { question: 'Can employees act without asking me?', answer: 'You choose. Permission modes are set per employee, and anything that needs your approval waits in the app. Unattended work never runs in bypass.' },
  { question: 'Are these real people?', answer: 'No. They’re AI employees with names, roles and instructions, running on your coding agents. The names and photos make a large team easy to keep track of.' },
  { question: 'Can I make my own employee?', answer: 'Yes. Choose a name and a role, write the instructions, and upload a photo. The CEO can also propose a new specialist when nobody on the list fits.' },
  { question: 'What do I need for Remote?', answer: 'Either Tailscale, or Retinue’s native link with a password. Every new device is approved on your desktop, and Remote is off until you turn it on.' },
  { question: 'Is there a Mac or Linux version?', answer: 'Retinue is a Windows app today.' },
]
