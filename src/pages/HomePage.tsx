import { DeviceSection } from '../components/home/DeviceSection'
import { FeatureSection } from '../components/home/FeatureSection'
import { HeroSection } from '../components/home/HeroSection'
import './HomePage.css'

export function HomePage({
  onOpenApiDocs,
}: {
  onOpenApiDocs: () => void
}) {
  return (
    <>
      <HeroSection onOpenApiDocs={onOpenApiDocs} />
      <FeatureSection />
      <DeviceSection />
    </>
  )
}
