import { CompanyStory, Control, HiringSimulator, IdeaToShipped, WithinReach, WorkingDay } from '@/components/sections/deferred'
import { Hero } from '@/components/sections/Hero'
import { MeetTheTeam } from '@/components/sections/MeetTheTeam'
import { Questions } from '@/components/sections/Questions'
import { CTA } from '@/components/ui/CTA'

// The home page is this list. A new feature is usually one more section here.
export default function HomePage() {
  return (
    <>
      <Hero />
      <CompanyStory />
      <HiringSimulator />
      <MeetTheTeam />
      <IdeaToShipped />
      <WorkingDay />
      <Control />
      <WithinReach />
      <Questions />
      <CTA title="Hire your first employee." body="For Windows. You’ll need Claude Code, Codex or OpenCode installed." />
    </>
  )
}
