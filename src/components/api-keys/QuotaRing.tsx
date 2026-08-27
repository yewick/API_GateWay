interface QuotaRingProps {
  used: number;
  limit: number; // > 0 时按比例渲染；由调用方处理 -1（不限）
  size?: number;
}

const colorFor = (pct: number): string =>
  pct >= 90 ? "var(--danger)" : pct >= 70 ? "var(--warning)" : "var(--accent)";

/**
 * 配额使用环形图（SVG，无图表库）。颜色随使用率变化：>90% 红、>70% 黄、其余主题色。
 */
export function QuotaRing({ used, limit, size = 40 }: QuotaRingProps) {
  const pct = limit <= 0 ? 0 : Math.min(100, Math.max(0, (used / limit) * 100));
  const stroke = 4;
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const dash = c - (pct / 100) * c;
  const color = colorFor(pct);

  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      className="flex-shrink-0"
      role="img"
      aria-label={`配额已使用 ${Math.round(pct)}%`}
    >
      {/* 轨道 */}
      <circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        fill="none"
        stroke="var(--bg-tertiary)"
        strokeWidth={stroke}
      />
      {/* 进度 */}
      <circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        fill="none"
        stroke={color}
        strokeWidth={stroke}
        strokeLinecap="round"
        strokeDasharray={c}
        strokeDashoffset={dash}
        transform={`rotate(-90 ${size / 2} ${size / 2})`}
        style={{ transition: "stroke-dashoffset 0.3s ease, stroke 0.3s ease" }}
      />
      {/* 中心百分比 */}
      <text
        x="50%"
        y="50%"
        dominantBaseline="central"
        textAnchor="middle"
        fill="var(--text-primary)"
        style={{ fontSize: size * 0.26, fontWeight: 600 }}
      >
        {Math.round(pct)}%
      </text>
    </svg>
  );
}
