type ProgressRingProps = {
  value?: number;
  size?: number;
  stroke?: number;
  children: React.ReactNode;
};

export default function ProgressRing({
  value = 0,
  size = 48,
  stroke = 4,
  children,
}: ProgressRingProps) {
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;

  return (
    <div className="relative flex-initial" style={{ width: size, height: size }}>
      <svg width={size} height={size} style={{ transform: 'rotate(-90deg)' }}>
        <title>Progress Ring</title>
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke="var(--chip)"
          strokeWidth={stroke}
        />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke="var(--accent)"
          strokeWidth={stroke}
          strokeDasharray={c}
          strokeDashoffset={c * (1 - value)}
          strokeLinecap="round"
          style={{ transition: 'stroke-dashoffset .5s var(--ease)' }}
        />
      </svg>
      <div
        className="absolute inset-0 flex items-center justify-center font-bold tabular-nums"
        style={{ fontSize: size * 0.26 }}
      >
        {children}
      </div>
    </div>
  );
}
