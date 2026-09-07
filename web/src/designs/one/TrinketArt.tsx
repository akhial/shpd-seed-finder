import { useLayoutEffect, useRef, useState } from "react";
import { itemArt } from "../../lib/sprites";
import { Sprite } from "./parts";

/** Fit the atlas sprite to its available box, retaining nearest-neighbor rendering. */
export function TrinketSprite({ cell, maximum }: { cell: number; maximum: number }) {
  const box = useRef<HTMLSpanElement>(null);
  const [size, setSize] = useState(maximum);
  useLayoutEffect(() => {
    const element = box.current;
    if (!element) return;
    const fit = () =>
      setSize(
        Math.max(1, Math.floor(Math.min(maximum, element.clientWidth, element.clientHeight))),
      );
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    return () => observer.disconnect();
  }, [maximum]);
  return (
    <span className="d1-trinket-art" ref={box}>
      <Sprite art={itemArt(cell)} size={size} />
    </span>
  );
}

/** Measure the actual rendered name so even the longest stays on one line. */
export function TrinketName({ name }: { name: string }) {
  const box = useRef<HTMLSpanElement>(null);
  const text = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    const element = box.current;
    const label = text.current;
    if (!element || !label) return;
    const fit = () => {
      label.style.fontSize = "11px";
      const width = label.getBoundingClientRect().width;
      if (width > element.clientWidth)
        label.style.fontSize = `${(11 * element.clientWidth) / width}px`;
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    return () => observer.disconnect();
  }, [name]);
  return (
    <span className="d1-trinket-name" ref={box}>
      <span ref={text}>{name}</span>
    </span>
  );
}
