// Loads a cut by name: its timeline, and the kit its shots and crops come
// from. The page and the renderer both load cuts through it.

export const CUTS = { desktop: "./timeline.js", tui: "./timeline-tui.js" };

export async function loadCut(name = "desktop") {
  const file = CUTS[name];
  if (!file) throw new Error(`no cut ${name}; there are ${Object.keys(CUTS).join(", ")}`);
  const cut = await import(new URL(file, import.meta.url).href);
  const kit = await import(new URL(`../${cut.KIT}/crops.js`, import.meta.url).href);
  return { name, cut, kit };
}
