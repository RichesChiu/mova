import type { ReactNode, SVGProps } from 'react'

export type HomeIconName =
  | 'arrowLeft'
  | 'bell'
  | 'chevronRight'
  | 'clock'
  | 'edit'
  | 'home'
  | 'libraries'
  | 'play'
  | 'scan'
  | 'search'
  | 'settings'
  | 'trash'

interface HomeIconProps extends SVGProps<SVGSVGElement> {
  name: HomeIconName
}

export const HomeIcon = ({ name, ...props }: HomeIconProps) => (
  <svg
    aria-hidden="true"
    fill="none"
    focusable="false"
    stroke="currentColor"
    strokeLinecap="round"
    strokeLinejoin="round"
    strokeWidth="1.8"
    viewBox="0 0 24 24"
    {...props}
  >
    {iconPaths[name]}
  </svg>
)

const iconPaths: Record<HomeIconName, ReactNode> = {
  arrowLeft: <path d="m15 6-6 6 6 6" />,
  bell: (
    <>
      <path d="M18 9.8c0-3.5-2.2-5.8-6-5.8s-6 2.3-6 5.8c0 5-2 5.8-2 5.8h16s-2-.8-2-5.8Z" />
      <path d="M9.7 18.5a2.5 2.5 0 0 0 4.6 0" />
    </>
  ),
  chevronRight: <path d="m9 6 6 6-6 6" />,
  clock: (
    <>
      <circle cx="12" cy="12" r="8" />
      <path d="M12 7.8V12l3 2" />
    </>
  ),
  edit: (
    <>
      <path d="M12 20h8" />
      <path d="m16.5 3.5 4 4L8 20l-4.5.5L4 16 16.5 3.5Z" />
    </>
  ),
  home: (
    <>
      <path d="m4.5 11 7.5-6 7.5 6" />
      <path d="M6.5 10.2v8.3h11v-8.3" />
      <path d="M10 18.5v-5h4v5" />
    </>
  ),
  libraries: (
    <>
      <rect height="12.5" rx="2" width="12.5" x="6.5" y="6.5" />
      <path d="M4.5 8.5v9a2 2 0 0 0 2 2h9" />
      <path d="M9.5 10h6" />
      <path d="M9.5 13h6" />
      <path d="M9.5 16h3.5" />
    </>
  ),
  play: <path d="M9 7.5v9l7-4.5-7-4.5Z" fill="currentColor" stroke="none" />,
  scan: (
    <>
      <path d="M4 7V5.5A1.5 1.5 0 0 1 5.5 4H7" />
      <path d="M17 4h1.5A1.5 1.5 0 0 1 20 5.5V7" />
      <path d="M20 17v1.5a1.5 1.5 0 0 1-1.5 1.5H17" />
      <path d="M7 20H5.5A1.5 1.5 0 0 1 4 18.5V17" />
      <path d="M7 12h10" />
    </>
  ),
  search: (
    <>
      <circle cx="10.8" cy="10.8" r="5.8" />
      <path d="m15.2 15.2 4.3 4.3" />
    </>
  ),
  settings: (
    <>
      <path d="M12 8.5a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7Z" />
      <path d="M19 12a7 7 0 0 0-.1-1.1l2-1.5-2-3.4-2.4 1a7 7 0 0 0-1.9-1.1L14.3 3h-4.6l-.3 2.9A7 7 0 0 0 7.5 7L5.1 6l-2 3.4 2 1.5A7 7 0 0 0 5 12c0 .4 0 .8.1 1.1l-2 1.5 2 3.4 2.4-1a7 7 0 0 0 1.9 1.1l.3 2.9h4.6l.3-2.9a7 7 0 0 0 1.9-1.1l2.4 1 2-3.4-2-1.5c.1-.3.1-.7.1-1.1Z" />
    </>
  ),
  trash: (
    <>
      <path d="M4 6h16" />
      <path d="M9 6V4h6v2" />
      <path d="m6.5 6 .8 14h9.4l.8-14" />
      <path d="M10 10v6" />
      <path d="M14 10v6" />
    </>
  ),
}
