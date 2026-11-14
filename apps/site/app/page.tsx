import { SmoothScroll } from '@/components/motion'
import { EmployeeDialog } from '@/components/employees'
import { Nav } from '@/components/sections/Nav'
import { Hero } from '@/components/sections/Hero'
import { Agents } from '@/components/sections/Agents'
import { Orchestration } from '@/components/sections/Orchestration'
import { Team } from '@/components/sections/Team'
import { Workday } from '@/components/sections/Workday'
import { Anywhere } from '@/components/sections/Anywhere'
import { Connect } from '@/components/sections/Connect'
import { Privacy } from '@/components/sections/Privacy'
import { Enterprise } from '@/components/sections/Enterprise'
import { Closing } from '@/components/sections/Closing'
import { Footer } from '@/components/sections/Footer'

// Sections alternate: a shader behind one, plain paper behind the next.
export default function Home() {
  return <>
    <SmoothScroll />
    <Nav />
    <main>
      <Hero />
      <Agents />
      <Orchestration />
      <Team />
      <Workday />
      <Anywhere />
      <Connect />
      <Privacy />
      <Enterprise />
      <Closing />
    </main>
    <Footer />
    <EmployeeDialog />
  </>
}
