# EmptyState

What a screen says when there is nothing on it yet: what will appear, and how to make the first of it.

**Anatomy.** `mu-empty`: centred in its region, at most 440 wide, `space-48` by `space-24` padding, its parts `space-16` apart. From the top: Kep's portrait (the *Kep* asset, `mu-kep-portrait`, `size-kep`, pixels kept square) where the whole window is empty; the title in `title-window` (`mu-empty-title`); one or two sentences in `text-quiet` (`mu-empty-text`); then the action, a primary **Button** with its key in the tooltip, and a quiet one if there is a second.

**Where.**
- *No projects yet* (first run): Kep, "Add a project", and the one primary button.
- *All clear* (Needs you with nothing waiting): no Kep, since the list goes on below with what works.
- *No agent on it yet* (a ticket): an inline box with a dashed `line-strong` frame, not this component.
- *The daemon is out of reach:* Kep away (`▼;ᴥ;▼`), a **NeedsYouBanner** of the `mu-banner--away` kind, and what Muster is doing about it.

Never say "Nothing here". Say what will be here.
