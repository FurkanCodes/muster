# FolderRow

One folder in the folder browser under the path field, when the person adds a project.

**Anatomy.** `mu-folder`: `size-control-md` (28px) high, `space-8` sides, `radius-md`, the name in `code`, with the letters typed so far lit in `accent` (`mark`). A git repository has a **Label** "git" at the end; a plain folder has none. Hidden folders show only once a `.` is typed.

**Behaviour.** The list shows the folders in the folder typed so far, narrowed by the name after its last `/`, ignoring case, with matches at the start first. Typing chooses the first match (`aria-selected`), ↑↓ choose another, ⇥ fills it in, and ⏎ adds the chosen folder, or else the path as typed. On a Mac, Desktop, Documents and Downloads are not looked into to mark them, since that makes macOS ask for access. Names with control characters are left out.
