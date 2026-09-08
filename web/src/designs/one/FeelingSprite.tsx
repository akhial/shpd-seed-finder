import iconsUrl from "../../assets/dungeon-icons.png";
import type { FloorFeeling } from "../../lib/wasm/types";

// Icons.getLarge(Feeling): eight 15×16 frames at y=64, spaced 16px apart.
const frames: Record<FloorFeeling, number> = {
  none: 0,
  chasm: 1,
  water: 2,
  grass: 3,
  dark: 4,
  large: 5,
  traps: 6,
  secrets: 7,
};

export function FeelingSprite({ feeling }: { feeling?: FloorFeeling }) {
  if (!feeling || feeling === "none" || !(feeling in frames)) return null;
  return (
    <span
      className="d1-floor-feeling"
      role="img"
      aria-label={`${feeling} floor`}
      style={{
        backgroundImage: `url(${iconsUrl})`,
        backgroundPosition: `-${frames[feeling] * 16}px -64px`,
      }}
    />
  );
}
