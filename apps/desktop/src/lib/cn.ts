/** Joins class names, leaving out what is empty or false: `cn("mu-row", selected && "is-selected")`. */
export function cn(...names: (string | false | null | undefined)[]): string {
  return names.filter(Boolean).join(" ");
}
