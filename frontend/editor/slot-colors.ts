import { modeColor, type Mode } from "../model";

const slotPalette = [
  0x77c9ef, 0xf28b81, 0xf7ca74, 0xbb9af0, 0x65d8b8, 0xf093cc, 0x9ba8f5,
  0xd0df70, 0xf5a66b, 0x79b1a6, 0x68a6f0, 0xcd8ed1, 0xe0cca5, 0xec716c,
];

function rgb(color: number) {
  return [color >> 16, (color >> 8) & 255, color & 255] as const;
}

export function colorChannels(color: number | null): Pick<Mode, "red" | "green" | "blue"> {
  const [red, green, blue] = color === null ? [-1, -1, -1] : rgb(color).map(value => value / 255);
  return { red, green, blue };
}

export function cellColorHex(mode: Mode) {
  return `#${modeColor(mode).toString(16).padStart(6, "0")}`;
}

export function nextSlotColor(genome: Mode[]) {
  const used = new Set(genome.map(modeColor));
  const channels = [...used].map(rgb);
  let best = -1;
  let separation = 0;
  for (const color of slotPalette) {
    if (used.has(color)) continue;
    const [r, g, b] = rgb(color);
    const distance = Math.min(...channels.map(([x, y, z]) => (x - r) ** 2 + (y - g) ** 2 + (z - b) ** 2));
    if (distance > separation) {
      best = color;
      separation = distance;
    }
  }
  if (best >= 0) return best;
  // The odd multiplier permutes all 4096 combinations of bright RGB levels.
  for (let index = 0; ; index++) {
    const sample = (index * 1493) & 0xfff;
    const color = ((96 + 10 * (sample >> 8)) << 16)
      | ((96 + 10 * ((sample >> 4) & 15)) << 8) | (96 + 10 * (sample & 15));
    if (!used.has(color)) return color;
  }
}
