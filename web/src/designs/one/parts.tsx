import type { CSSProperties, ReactNode } from "react";
import type { Glow } from "../../lib/glow";
import { nearestOptionIndex } from "../../lib/query";
import type { ItemArt } from "../../lib/sprites";
import { ringIconCss, spriteBoxCss, spriteGlowCss } from "../../lib/sprites";

export function Sprite({
  art,
  size = 24,
  label,
  glow,
}: {
  /**
   * What to draw, from `itemArt`. It is an `ItemArt` rather than a bare sheet
   * index so that callers have to say whether they hold a run: pass the scout
   * document's `ringGems` and a ring shows the gem that seed gave it, omit them
   * and it shows the catalog's per-class cell.
   */
  art: ItemArt;
  size?: number;
  label?: string;
  /**
   * Enchantment/curse glow that pulses the icon, matching the game. Several
   * glows pulse in turn, one after another.
   */
  glow?: Glow | Glow[] | null;
}) {
  const box = spriteBoxCss(art.cell, size);
  const ringIcon = ringIconCss(art.ringGlyph, size);
  const glows = glow ? (Array.isArray(glow) ? glow : [glow]) : [];
  return (
    <span
      className="d1-sprite"
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      style={box.outer}
    >
      <span style={box.inner}>
        {glows.length > 0 && (
          <span
            className={glows.length > 1 ? "d1-sprite-glow d1-sprite-glow-seq" : "d1-sprite-glow"}
            style={spriteGlowCss(art.cell, size, glows)}
          />
        )}
      </span>
      {ringIcon && <span style={ringIcon} />}
    </span>
  );
}

export interface SegmentedOption<T> {
  value: T;
  label: string;
}

export function Segmented<T extends string | number>({
  value,
  options,
  onChange,
  ariaLabel,
  fill,
}: {
  value: T;
  options: SegmentedOption<T>[];
  onChange: (value: T) => void;
  ariaLabel?: string;
  fill?: boolean;
}) {
  return (
    <div className={fill ? "d1-seg d1-seg-fill" : "d1-seg"} role="group" aria-label={ariaLabel}>
      {options.map((option) => (
        <button
          type="button"
          key={String(option.value)}
          className={option.value === value ? "d1-seg-on" : undefined}
          aria-pressed={option.value === value}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function Stepper({
  value,
  min,
  max,
  onChange,
  ariaLabel,
  format,
}: {
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
  ariaLabel: string;
  format?: (value: number) => string;
}) {
  return (
    <div className="d1-stepper" role="group" aria-label={ariaLabel}>
      <button
        type="button"
        aria-label="One fewer"
        disabled={value <= min}
        onClick={() => onChange(value - 1)}
      >
        −
      </button>
      <span className="d1-stepper-value d1-mono" aria-live="polite">
        {format ? format(value) : String(value)}
      </span>
      <button
        type="button"
        aria-label="One more"
        disabled={value >= max}
        onClick={() => onChange(value + 1)}
      >
        +
      </button>
    </div>
  );
}

export function Field({
  label,
  stack,
  children,
}: {
  label: string;
  /** Let a wide control drop under its label on narrow screens. */
  stack?: boolean;
  children: ReactNode;
}) {
  return (
    <div className={stack ? "d1-field d1-field-stack" : "d1-field"}>
      <span className="d1-field-label">{label}</span>
      <div className="d1-field-control">{children}</div>
    </div>
  );
}

type SliderRowScale =
  /** Explicit selectable values (e.g. floor limits that skip empty boss floors). */
  | { values: readonly number[]; min?: undefined; max?: undefined }
  | { values?: undefined; min: number; max: number };

export function SliderRow(
  props: {
    label: string;
    valueLabel: string;
    value: number;
    onChange: (value: number) => void;
    /** Fill the track left of the thumb — for "first N floors" style ranges. */
    fill?: boolean;
  } & SliderRowScale,
) {
  const { label, valueLabel, value, onChange, fill = false } = props;
  const options =
    props.values !== undefined
      ? props.values
      : Array.from({ length: props.max - props.min + 1 }, (_, index) => props.min + index);
  // Off-list values (e.g. a stored floor limit of an empty boss floor) snap to the nearest option below.
  const index = nearestOptionIndex(options, value);
  const percent = (index / (options.length - 1)) * 100;
  return (
    <div className="d1-slider">
      <div className="d1-slider-head">
        <span>{label}</span>
        <span className="d1-mono d1-slider-value">{valueLabel}</span>
      </div>
      <input
        type="range"
        className={fill ? "d1-range-fill" : undefined}
        style={{ "--d1-range-percent": `${percent}%` } as CSSProperties}
        min={0}
        max={options.length - 1}
        step={1}
        value={index}
        aria-label={label}
        aria-valuetext={String(options[index])}
        onChange={(event) => onChange(options[Number(event.currentTarget.value)])}
      />
      <div className="d1-slider-ticks" aria-hidden="true">
        {options.map((option, tick) => (
          <span key={option} className={tick <= index && fill ? "d1-tick-passed" : undefined} />
        ))}
      </div>
    </div>
  );
}
