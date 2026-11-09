// Site-wide facts and navigation. A new page or section usually starts here.
export const site = {
  name: 'Retinue',
  tagline: 'Your company of AI employees.',
  description:
    'Retinue is a Windows app that runs like a company: a CEO, specialists hired into your projects, and you as the owner. It runs on the coding agents you already use.',
  /** The Windows installer. While empty, download buttons go to the download section. */
  downloadUrl: '',
  /** The open-source repository (MIT). */
  repoUrl: 'https://github.com/sico-vibes/retinue',
}

/** SignPath Foundation's terms ask for this policy to be linked from the home and download pages. */
export const codeSigningPolicyUrl = `${site.repoUrl}#code-signing-policy`
export const privacyUrl = `${site.repoUrl}/blob/main/PRIVACY.md`

export const downloadHref = site.downloadUrl || '/#download'

export interface LinkItem {
  label: string
  href: string
}

export const navLinks: LinkItem[] = [
  { label: 'How it works', href: '/#company' },
  { label: 'Hiring', href: '/#hiring' },
  { label: 'Team', href: '/team/' },
  { label: 'Working day', href: '/#day' },
  { label: 'Control', href: '/#control' },
  { label: 'Questions', href: '/#questions' },
]

export const footerColumns: { title: string; links: LinkItem[] }[] = [
  {
    title: 'Product',
    links: [
      { label: 'How it works', href: '/#company' },
      { label: 'Hiring', href: '/#hiring' },
      { label: 'From idea to shipped', href: '/#shipped' },
      { label: 'The working day', href: '/#day' },
    ],
  },
  {
    title: 'Your team',
    links: [
      { label: 'Meet the specialists', href: '/team/' },
      { label: 'Companion and dictation', href: '/#reach' },
      { label: 'Remote', href: '/#reach' },
    ],
  },
  {
    title: 'More',
    links: [
      { label: 'Privacy and control', href: '/#control' },
      { label: 'Questions', href: '/#questions' },
      { label: 'Download', href: downloadHref },
      { label: 'Source code', href: site.repoUrl },
      { label: 'Code signing policy', href: codeSigningPolicyUrl },
      { label: 'Privacy', href: privacyUrl },
    ],
  },
]
