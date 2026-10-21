# PullRequest

The pull request from a worktree's branch, as its git host last said: its number, how its review stands, and its checks.

**Anatomy.** `mu-pr`, in `meta`: `#88`, then one word, then the checks' mark, `space-6` apart.

| Word | Colour | When |
|---|---|---|
| draft | `text-quiet` | a draft |
| open | `text-quiet` | open, no review yet |
| review | `working` | a review was asked for |
| approved | `done` | approved |
| changes | `waiting` | changes were requested |
| merged | `merged` | merged |
| closed | `text-quiet` | closed without merging |

Checks: ✓ passed (`done`), ✗ failed (`waiting`), ◌ running (`working`). Its tooltip says it all in words ("#88, open, checks passed, approved"), and `b` opens it in the browser.

- Short of room, the word goes first, then the checks; the number always stays.
- Muster only reads the pull request. Nothing here changes it.
