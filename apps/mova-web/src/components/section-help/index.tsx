interface SectionHelpProps {
  detail: string
  title: string
}

// Keep dense section headers readable while preserving a place for longer guidance.
export const SectionHelp = ({ detail, title }: SectionHelpProps) => (
  <span className="section-help">
    <button aria-label={title} className="section-help__trigger" type="button">
      <svg aria-hidden="true" fill="none" focusable="false" viewBox="0 0 20 20">
        <circle cx="10" cy="10" r="8.25" stroke="currentColor" strokeWidth="1.5" />
        <path
          d="M7.9 7.55C8.14 6.56 8.97 5.9 10.04 5.9C11.31 5.9 12.16 6.71 12.16 7.81C12.16 8.66 11.73 9.14 10.82 9.71C10.06 10.18 9.74 10.56 9.74 11.3V11.56"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="1.5"
        />
        <circle cx="9.98" cy="14.18" fill="currentColor" r="0.9" />
      </svg>
    </button>
    <span className="section-help__tooltip" role="tooltip">
      {detail}
    </span>
  </span>
)
