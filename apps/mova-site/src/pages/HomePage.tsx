import { DeviceSection } from '../components/home/DeviceSection'
import { FeatureSection } from '../components/home/FeatureSection'
import { HeroSection } from '../components/home/HeroSection'
import './HomePage.scss'

export function HomePage({
  onOpenDeployment,
  onOpenApiDocs,
}: {
  onOpenDeployment: () => void
  onOpenApiDocs: () => void
}) {
  return (
    <>
      <HeroSection onOpenDeployment={onOpenDeployment} onOpenApiDocs={onOpenApiDocs} />
      <FeatureSection />
      <DeviceSection />
    </>
  )
}
