import { Container, Heading, Section } from '@/components/ui/layout'
import { Reveal } from '@/components/ui/Reveal'
import { faq } from '@/content/faq'

export function Questions() {
  return (
    <Section id="questions">
      <Container narrow className="grid gap-10 md:grid-cols-[minmax(0,4fr)_minmax(0,8fr)]">
        <Reveal><Heading>Questions</Heading></Reveal>
        <Reveal delay={0.08} className="border-t border-line-strong">
          {faq.map((item) => (
            <details key={item.question} className="group border-b border-line-strong">
              <summary className="relative cursor-pointer list-none py-5 pr-9 text-[17px] font-medium tracking-[-0.012em]">
                {item.question}
                <span aria-hidden="true" className="absolute top-[28px] right-1.5 h-2 w-2 rotate-45 border-r-[1.5px] border-b-[1.5px] border-ink-2 transition-transform duration-200 group-open:translate-y-1 group-open:rotate-[-135deg]" />
              </summary>
              <p className="pr-9 pb-6 leading-relaxed text-ink-2">{item.answer}</p>
            </details>
          ))}
        </Reveal>
      </Container>
    </Section>
  )
}
