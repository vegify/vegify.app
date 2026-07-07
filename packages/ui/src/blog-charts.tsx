import { type ReactNode, useEffect, useState } from "react"
import {
  Bar,
  BarChart,
  LabelList,
  ReferenceLine,
  ResponsiveContainer,
  XAxis,
  YAxis
} from "recharts"

/**
 * Blog charts (Recharts). These illustrate the DRI explainer and are the first of the shadcn-style
 * chart components in @vegify/ui — the targets/profiles feature will grow more here. Colors come from
 * the Tailwind v4 @theme CSS vars (brand green + orange), so they track light/dark automatically.
 *
 * CLIENT-ONLY: Recharts' ResponsiveContainer measures the DOM and throws under SSR (which would abort
 * the whole post body). So each chart renders a fixed-height placeholder on the server + first client
 * render (no hydration mismatch), then swaps in the chart after mount. The post prose + each figure's
 * caption state every number, so a crawler / no-JS reader loses none of the content.
 */
function ChartFrame({
  height,
  children
}: {
  height: number
  children: ReactNode
}) {
  const [mounted, setMounted] = useState(false)
  useEffect(() => setMounted(true), [])
  if (!mounted) return <div style={{ height }} aria-hidden />
  return (
    <ResponsiveContainer width="100%" height={height}>
      {children as React.ReactElement}
    </ResponsiveContainer>
  )
}

const GREEN = "var(--color-green-dark, #2f5233)"
const GREEN_SOFT = "var(--color-green, #4a7c3f)"
const MUTED = "var(--color-muted-foreground, #6b7280)"
const ORANGE = "var(--color-orange, #e07a3f)"
const axisTick = {
  fontSize: 12,
  fill: "var(--color-muted-foreground, #6b7280)"
}

/**
 * The three-band model on one nutrient: a horizontal bar split into "up to the target" (muted) and
 * "target → ceiling" headroom (green). The x-axis runs 0 → ceiling, so a nutrient with lots of
 * headroom (vitamin C: aim 90, ceiling 2000) shows the target as a sliver and a vast green safe zone,
 * while a tight one (zinc) fills most of the bar before the ceiling. That contrast IS the lesson.
 */
export function NutrientRangeChart({
  name,
  target,
  ceiling,
  unit
}: {
  name: string
  target: number
  ceiling: number
  unit: string
}) {
  const data = [{ name, target, headroom: Math.max(ceiling - target, 0) }]
  return (
    <ChartFrame height={92}>
      <BarChart
        layout="vertical"
        data={data}
        margin={{ top: 4, right: 16, bottom: 4, left: 8 }}
      >
        <XAxis
          type="number"
          domain={[0, ceiling]}
          tick={axisTick}
          tickFormatter={(v: number) => `${v.toLocaleString()}`}
          unit={` ${unit}`}
        />
        <YAxis type="category" dataKey="name" hide />
        <Bar
          dataKey="target"
          stackId="a"
          fill={MUTED}
          radius={[4, 0, 0, 4]}
          isAnimationActive={false}
        >
          <LabelList
            dataKey="target"
            position="insideRight"
            fill="#fff"
            fontSize={12}
            formatter={(v) => `aim ${v}`}
          />
        </Bar>
        <Bar
          dataKey="headroom"
          stackId="a"
          fill={GREEN}
          radius={[0, 4, 4, 0]}
          isAnimationActive={false}
        >
          <LabelList
            dataKey="headroom"
            position="insideRight"
            fill="#fff"
            fontSize={12}
            formatter={() =>
              `safe range → ceiling ${ceiling.toLocaleString()} ${unit}`
            }
          />
        </Bar>
      </BarChart>
    </ChartFrame>
  )
}

/** A bar per group for one nutrient. With `ceiling` set, the shared UL is drawn as a reference line
 *  (the "one %DV can't fit everyone" case); without it, it's a plain comparison (e.g. US RDA vs EFSA
 *  PRI, where there's no shared ceiling to show) and the axis just fits the bars. */
export function NutrientByGroupChart({
  unit,
  ceiling,
  groups
}: {
  unit: string
  ceiling?: number
  groups: { label: string; value: number }[]
}) {
  const maxValue = Math.max(...groups.map((g) => g.value), ceiling ?? 0)
  const domainMax = Math.ceil(ceiling ? maxValue * 1.05 : maxValue * 1.18)
  return (
    <ChartFrame height={groups.length * 44 + 40}>
      <BarChart
        layout="vertical"
        data={groups}
        margin={{ top: 4, right: 56, bottom: 4, left: 8 }}
      >
        <XAxis
          type="number"
          domain={[0, domainMax]}
          tick={axisTick}
          unit={` ${unit}`}
        />
        <YAxis type="category" dataKey="label" tick={axisTick} width={104} />
        {ceiling != null && (
          <ReferenceLine
            x={ceiling}
            stroke={ORANGE}
            strokeDasharray="4 3"
            label={{
              value: `ceiling ${ceiling} ${unit}`,
              position: "top",
              fill: ORANGE,
              fontSize: 11
            }}
          />
        )}
        <Bar
          dataKey="value"
          fill={GREEN_SOFT}
          radius={[0, 4, 4, 0]}
          isAnimationActive={false}
        >
          <LabelList
            dataKey="value"
            position="right"
            fill="var(--color-foreground, #1a1a1a)"
            fontSize={12}
            formatter={(v) => `${v} ${unit}`}
          />
        </Bar>
      </BarChart>
    </ChartFrame>
  )
}
