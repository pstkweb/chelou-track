// Teinte stable dérivée du titre. Hash à avalanche (xorshift) + angle d'or :
// de petites différences de titre écartent fortement la teinte → pas de quasi-collisions.
export function computeMethodColors(title: string): [string, string] {
  let h = 0x811c9dc5;

  for (let i = 0; i < title.length; i++) {
    h ^= title.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
    h ^= h >>> 7;
  }
  h = h >>> 0;

  const hue = (h * 137.508) % 360; // angle d'or → répartition maximale
  const chroma = 0.12 + (((h >>> 8) % 100) / 100) * 0.05; // 0.12–0.17
  const light = 0.6 + (((h >>> 16) % 100) / 100) * 0.08; // 0.60–0.68

  return [
    `oklch(${light.toFixed(3)} ${chroma.toFixed(3)} ${hue})`,
    `oklch(${(light - 0.22).toFixed(3)} ${(chroma - 0.03).toFixed(3)} ${(hue + 18) % 360})`,
  ];
}
