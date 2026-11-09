// The hiring simulation. Add a project type here and it appears in the picker.
export interface HiringScenario {
  id: string
  label: string
  prompt: string
  ceo: string
  team: string[]
  specialist: { name: string; role: string; why: string }
}

export const hiringScenarios: HiringScenario[] = [
  {
    id: 'shop',
    label: 'An online shop',
    prompt: 'I want to sell my pottery online.',
    ceo: 'You need payments that work, pages that feel handmade, and someone checking every change. Here’s who I’d hire.',
    team: ['ada', 'fifi', 'jo', 'nubi'],
    specialist: { name: 'Paz', role: 'Product photo editor', why: 'Nobody on the list prepares product photos, so I’d add a specialist.' },
  },
  {
    id: 'mobile',
    label: 'A mobile app',
    prompt: 'I want an app for booking climbing sessions.',
    ceo: 'Small screens, patchy signal and store reviews are the hard parts. This team covers them.',
    team: ['nova', 'wu', 'lan', 'ludo'],
    specialist: { name: 'Tess', role: 'App store writer', why: 'The listing decides who installs it, and nobody here specialises in that.' },
  },
  {
    id: 'data',
    label: 'A data pipeline',
    prompt: 'Our sales data lives in six spreadsheets.',
    ceo: 'First we move it safely, then we make it trustworthy, then we make it useful.',
    team: ['art', 'vlad', 'mira', 'sleuth'],
    specialist: { name: 'Reva', role: 'Data privacy analyst', why: 'There’s customer data in those sheets, so I’d want someone watching how it’s handled.' },
  },
  {
    id: 'docs',
    label: 'A docs site',
    prompt: 'Our product needs proper documentation.',
    ceo: 'Clear writing, a site anyone can use, and a way to publish it without fuss.',
    team: ['will', 'ludo', 'volt', 'cleo'],
    specialist: { name: 'Lin', role: 'Translator', why: 'You mentioned customers abroad, and nobody on the list translates.' },
  },
  {
    id: 'backend',
    label: 'A SaaS backend',
    prompt: 'I’m building subscription software for dentists.',
    ceo: 'Get the design right first, then build it securely and keep it running.',
    team: ['bert', 'ada', 'jo', 'volt'],
    specialist: { name: 'Cass', role: 'Billing specialist', why: 'Subscriptions have awkward edge cases, so I’d add someone who knows them.' },
  },
]
