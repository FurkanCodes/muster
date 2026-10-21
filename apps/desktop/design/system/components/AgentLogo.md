# AgentLogo

Which coding agent a session is, by its own logo, after the status mark.

**Sizes.** `mu-logo--sm` is `size-logo-sm` (16px), for turns and menus. The default is `size-logo` (18px), for rows. `mu-logo--lg` is `size-logo-lg` (24px), beside the pane's title. Corners are `radius-xs`, or `radius-sm` at 24px.

**Images.** Use the *Agents* assets as they are. Claude's and Gemini's marks sit on a clear ground; Codex's, Copilot's and OpenCode's come on their own dark tile.

**Fallback.** Where a logo cannot be drawn (it failed to load, or the agent was added by the person), show its mark (`mu-logo--mark`) on `surface-raised` in its `agent-*` colour: ✻ Claude Code, ◎ Codex, ✦ Gemini CLI, ▣ OpenCode, ◉ Copilot CLI, • an added agent. A shell is ❯ in `text-quiet`.

**The consumer provides** the agent, and an `alt` naming it ("Codex"), since it is the only place the row says which agent this is.

- Don't: recolour, outline or enlarge a logo past 24px.
