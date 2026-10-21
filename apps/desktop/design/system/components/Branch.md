# Branch

Where a session runs: its branch, and its project in the project's colour.

**Anatomy.** `mu-branch`: ⎇ and the branch in `code` (JetBrains Mono 12px) in `text`. Then `mu-where`: "in" in `text-quiet` and the project's name in its `project-*` colour (`mu-p1`…`mu-p6`). A session in the project's own checkout reads its checked-out branch; in a plain folder, the folder's name with no ⎇.

**Cutting.** A long branch is cut in the middle, keeping its start and its end (`muster/issue-212…cache`), since the end usually names the work. The project's name is never cut.

- Don't: colour the branch itself, or put the project's colour on anything but its name and swatch.
