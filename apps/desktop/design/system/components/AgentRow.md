# AgentRow

One coding agent on Settings › Agents: whether Muster finds it, and what it can do there.

**Anatomy.** `mu-agentrow`: `space-12` padding, `radius-md`, hover `surface-raised`. It holds:
- the agent's logo at `size-logo-lg`;
- the name in `body` at 600, its version in `code` and `text-quiet`, and a **Label** for one the person added ("added by you");
- under it (`mu-agentrow-facts`, `meta`, `space-12` apart) what Muster knows of it: where its program is ✓, or ✗ "not found" in `waiting` with how to get it, then what it can do with Muster. "status from its hooks", "resumes after a restart", "takes a first prompt" and "can name chats" each read ✓ in `done` or ✗ in `text-quiet`. Where it is heard with less detail, say how ("status from its window title").

**Added agents** come from `[agents.<name>]` in Muster's own `settings.toml`, never a repository's. One that does not read shows the line that does not read and why, in `waiting`, and is offered nowhere until it is fixed and the file is read again (`r`).
