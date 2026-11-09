import { DownloadButton } from './Button'
import { Container, Heading, Lead } from './layout'
import { Reveal } from './Reveal'

/** The closing invitation. Sits directly above the footer art. */
export function CTA({ title, body }: { title: string; body: string }) {
  return (
    <section id="download" className="relative z-10 pt-24 pb-14 sm:pt-36 sm:pb-20">
      <Container className="text-center">
        <Reveal>
          <Heading size="display" className="mx-auto max-w-[12ch]">{title}</Heading>
          <Lead className="mx-auto mt-7 max-w-[520px]">{body}</Lead>
          <DownloadButton className="mt-9" />
        </Reveal>
      </Container>
    </section>
  )
}
