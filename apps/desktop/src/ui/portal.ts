// Where what floats is drawn (menus, tooltips): inside the window's root,
// so it takes the design system's face, colours and box sizing (.mu-root),
// as everything else in the window does; the page's body where there is none.

export function portalRoot(): HTMLElement | undefined {
  return document.querySelector<HTMLElement>(".mu-root") ?? undefined;
}
