import Image, { getImageProps } from 'next/image'
import { ButtonLink, DownloadButton } from '@/components/ui/Button'
import { Container, Lead } from '@/components/ui/layout'
import { ProductFrame, Shot } from '@/components/ui/ProductFrame'
import { agentNames } from '@/content/employees'
import { site } from '@/content/site'

const { props: wideArt } = getImageProps({ src: '/art/hero.webp', alt: '', fill: true, sizes: '100vw' })

const headline = ['Your', 'company', 'of', 'AI', 'employees.']

// Who is sitting where in the hero picture, as a percentage of its width and height.
const seats: { name: string; role: string; state?: string; x: number; y: number; w: number; h: number }[] = [
  { name: 'Amy', role: 'Release manager', x: 0.5, y: 28, w: 12, h: 48 },
  { name: 'Charlie', role: 'QA and usability', x: 8, y: 6, w: 8, h: 28 },
  { name: 'Nova', role: 'Mobile engineer', x: 16, y: 6, w: 12, h: 30 },
  { name: 'Ace', role: 'On-call', state: 'Working', x: 15, y: 34, w: 16, h: 44 },
  { name: 'Chief', role: 'APIs and integrations', x: 29, y: 46, w: 11, h: 34 },
  { name: 'Zip', role: 'Performance', state: 'Needs you', x: 68, y: 44, w: 11, h: 40 },
  { name: 'Ludo', role: 'Accessibility', x: 78, y: 47, w: 12, h: 42 },
  { name: 'Cleo', role: 'Marketing and growth', x: 77, y: 16, w: 11, h: 30 },
  { name: 'Brute', role: 'Load and stress tests', x: 88, y: 4, w: 11, h: 30 },
  { name: 'Will', role: 'Copy and docs', state: 'Done', x: 89, y: 33, w: 11, h: 40 },
]

function SeatTag({ seat }: { seat: (typeof seats)[number] }) {
  const tone = seat.state === 'Done' ? 'bg-green' : 'bg-amber status-pulse'
  return (
    <div className="group absolute" style={{ left: `${seat.x}%`, top: `${seat.y}%`, width: `${seat.w}%`, height: `${seat.h}%` }}>
      <div className={`glass frost absolute top-0 left-1/2 flex -translate-x-1/2 items-center gap-2 rounded-lg px-2.5 py-1.5 text-xs whitespace-nowrap shadow-soft transition-opacity duration-300 ${seat.state ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'}`}>
        {seat.state && <span className={`h-1.5 w-1.5 rounded-full ${tone}`} />}
        <span className="font-medium">{seat.name}</span>
        <span className="text-ink-2">{seat.state ?? seat.role}</span>
      </div>
    </div>
  )
}

export function Hero() {
  return (
    <section className="relative">
      <div className="relative flex flex-col lg:block lg:min-h-[100svh]">
        <Container className="relative z-10 order-1 pt-[112px] text-center sm:pt-[132px] lg:pt-[clamp(118px,14vh,156px)]">
          <h1 className="mx-auto max-w-[11ch] text-[clamp(2.75rem,7vw,6.25rem)] leading-[0.99] font-medium tracking-[-0.045em] text-balance">
            <span className="sr-only">{site.tagline}</span>
            <span aria-hidden="true">
              {headline.map((word, index) => <span key={word}><span className="word" style={{ '--i': index } as React.CSSProperties}>{word}</span>{index < headline.length - 1 ? ' ' : ''}</span>)}
            </span>
          </h1>
          <div className="rise" style={{ '--delay': '420ms' } as React.CSSProperties}>
            <Lead className="mx-auto mt-5 max-w-[30em] text-balance sm:mt-6">Retinue is a Windows app that runs like a company. A CEO hires specialists into your projects, on the coding agents you already use. You stay the owner.</Lead>
            <div className="mt-6 flex flex-wrap justify-center gap-3 sm:mt-7">
              <DownloadButton />
              <ButtonLink href="/#company" look="ghost">See how it works</ButtonLink>
            </div>
          </div>
        </Container>

        <div className="hero-art hero-parallax relative order-2 lg:absolute lg:inset-x-0 lg:bottom-0">
          <div className="relative aspect-[4/3] w-full sm:aspect-[1920/819]">
            {/* Phones get their own crop: lighter, and sharper than a wide image cut down to fit. */}
            <picture>
              <source media="(min-width: 640px)" srcSet={wideArt.srcSet} sizes="100vw" />
              <Image unoptimized src="/art/hero-mobile.webp" alt="The Retinue team in a boardroom at dusk, with a city skyline through the window." fill fetchPriority="high" loading="eager" className="object-cover" />
            </picture>
            <div aria-hidden="true" className="absolute inset-0 hidden lg:block">
              {seats.map((seat) => <SeatTag key={seat.name} seat={seat} />)}
            </div>
          </div>
        </div>
      </div>

      <Container className="relative z-20 max-lg:-mt-[12vw] lg:-mt-[11vh]">
        <div className="frame-settle">
          <ProductFrame title="Retinue">
            <Shot name="main" width={1920} height={1200} sizes="(min-width: 1280px) 1200px, 96vw" alt="The Retinue window. A sidebar lists projects and employees, and Ada, a backend engineer, reports what Fifi, a UI designer, found." />
          </ProductFrame>
        </div>
        <p className="mt-10 text-center text-[15px] text-ink-3 sm:mt-14">
          Runs on the coding agents you already have:{' '}
          {Object.values(agentNames).map((name, index, all) => <span key={name}><span className="font-medium text-ink-2">{name}</span>{index < all.length - 2 ? ', ' : index === all.length - 2 ? ' and ' : '.'}</span>)}
        </p>
      </Container>
    </section>
  )
}
