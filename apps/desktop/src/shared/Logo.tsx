/** The Upscalee mark: low-res pixels growing into a smooth, larger image. */
export function Logo({ size = 24, title }: { size?: number; title?: string }) {
  const id = `logo-${size}`;
  return (
    <svg width={size} height={size} viewBox="0 0 1024 1024" role={title ? "img" : undefined} aria-hidden={!title}>
      {title && <title>{title}</title>}
      <defs>
        <linearGradient id={id} x1="0" y1="1" x2="1" y2="0">
          <stop offset="0" stopColor="#5B3DF5" />
          <stop offset="1" stopColor="#1FB8F0" />
        </linearGradient>
      </defs>
      <rect x="32" y="32" width="960" height="960" rx="224" fill={`url(#${id})`} />
      <g fill="#fff">
        <rect x="216" y="616" width="84" height="84" rx="16" opacity="0.5" />
        <rect x="316" y="616" width="84" height="84" rx="16" opacity="0.75" />
        <rect x="216" y="716" width="84" height="84" rx="16" opacity="0.75" />
        <rect x="316" y="716" width="84" height="84" rx="16" />
      </g>
      <path d="M470 554 L716 308" fill="none" stroke="#fff" strokeWidth="92" strokeLinecap="round" />
      <path
        d="M528 288 H736 V496"
        fill="none"
        stroke="#fff"
        strokeWidth="92"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
