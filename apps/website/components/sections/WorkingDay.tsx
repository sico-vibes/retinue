import { LiveWindow } from '@/components/mock/LiveWindow'
import { featureVisuals } from '@/components/mock/visuals'
import { FeatureCard } from '@/components/ui/FeatureCard'
import { Container, Section, SectionHead } from '@/components/ui/layout'
import { Reveal } from '@/components/ui/Reveal'
import { features } from '@/content/features'
import { MotionProvider } from '@/components/ui/Motion'

export function WorkingDay() {
  return (
    <Section id="day">
      <Container>
        <Reveal>
          <SectionHead eyebrow="The working day" title="Work happens in conversations." lead="Replies stream in as they’re written. When an employee needs permission, the question appears right there and you answer it. Try it." />
        </Reveal>
        <Reveal><MotionProvider><LiveWindow /></MotionProvider></Reveal>

        <div className="mt-20 grid gap-x-6 gap-y-16 sm:mt-28 md:grid-cols-2">
          {features.map((feature, index) => {
            const Visual = featureVisuals[feature.visual]
            return (
              <Reveal key={feature.title} className={feature.wide ? 'md:col-span-2' : ''}>
                <FeatureCard index={index + 1} title={feature.title} body={feature.body}><Visual /></FeatureCard>
              </Reveal>
            )
          })}
        </div>
      </Container>
    </Section>
  )
}
