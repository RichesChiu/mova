import { CommunitySection } from '../components/home/CommunitySection'
import { DeploySection } from '../components/home/DeploySection'
import { DeviceSection } from '../components/home/DeviceSection'
import { DocsSection } from '../components/home/DocsSection'
import { FeatureSection } from '../components/home/FeatureSection'
import { HeroSection } from '../components/home/HeroSection'

export function HomePage({
  onNavigate,
  onOpenApiDocs,
}: {
  onNavigate: (sectionId: string) => void
  onOpenApiDocs: () => void
}) {
  return (
    <>
      <HeroSection onNavigate={onNavigate} onOpenApiDocs={onOpenApiDocs} />
      <FeatureSection />
      <DeviceSection />
      <DeploySection />
      <DocsSection />
      <CommunitySection />
    </>
  )
}
