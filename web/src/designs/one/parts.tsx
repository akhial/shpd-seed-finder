import type { CSSProperties, ReactNode } from 'react'
import type { Glow } from '../../lib/glow'
import { ringIconCss, spriteBoxCss, spriteGlowCss } from '../../lib/sprites'

export function Sprite({
  index,
  size = 24,
  label,
  glow,
}: {
  index: number
  size?: number
  label?: string
  /** Enchantment/curse glow that pulses the icon, matching the game. */
  glow?: Glow | null
}) {
  const box = spriteBoxCss(index, size)
  const ringIcon = ringIconCss(index, size)
  return (
    <span
      className="d1-sprite"
      role={label ? 'img' : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      style={box.outer}
    >
      <span style={box.inner}>
        {glow && <span className="d1-sprite-glow" style={spriteGlowCss(index, size, glow.color, glow.period)} />}
      </span>
      {ringIcon && <span style={ringIcon} />}
    </span>
  )
}

export interface SegmentedOption<T> { value: T; label: string }

export function Segmented<T extends string | number>({
  value,
  options,
  onChange,
  ariaLabel,
  fill,
}: {
  value: T
  options: SegmentedOption<T>[]
  onChange: (value: T) => void
  ariaLabel?: string
  fill?: boolean
}) {
  return (
    <div className={fill ? 'd1-seg d1-seg-fill' : 'd1-seg'} role="group" aria-label={ariaLabel}>
      {options.map((option) => (
        <button
          type="button"
          key={String(option.value)}
          className={option.value === value ? 'd1-seg-on' : undefined}
          aria-pressed={option.value === value}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  )
}

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="d1-field">
      <span className="d1-field-label">{label}</span>
      <div className="d1-field-control">{children}</div>
    </div>
  )
}

export function SliderRow({
  label,
  valueLabel,
  min,
  max,
  value,
  onChange,
  fill = false,
}: {
  label: string
  valueLabel: string
  min: number
  max: number
  value: number
  onChange: (value: number) => void
  /** Fill the track left of the thumb — for "first N floors" style ranges. */
  fill?: boolean
}) {
  const percent = ((value - min) / (max - min)) * 100
  return (
    <div className="d1-slider">
      <div className="d1-slider-head">
        <span>{label}</span>
        <span className="d1-mono d1-slider-value">{valueLabel}</span>
      </div>
      <input
        type="range"
        className={fill ? 'd1-range-fill' : undefined}
        style={{ '--d1-range-percent': `${percent}%` } as CSSProperties}
        min={min}
        max={max}
        step={1}
        value={value}
        aria-label={label}
        onChange={(event) => onChange(Number(event.currentTarget.value))}
      />
      <div className="d1-slider-ticks" aria-hidden="true">
        {Array.from({ length: max - min + 1 }, (_, index) => (
          <span key={index} className={min + index <= value && fill ? 'd1-tick-passed' : undefined} />
        ))}
      </div>
    </div>
  )
}
