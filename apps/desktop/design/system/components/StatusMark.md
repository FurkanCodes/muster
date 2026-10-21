# StatusMark

How a session stands, as one glyph in a column of its own at the start of its row.

**Anatomy.** `mu-mark` is a `size-mark` (12px) column, so names line up whatever the mark, set in `font-mono`:

| Mark | Class | Colour | Means |
|---|---|---|---|
| ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ | `mu-mark--working` | `working` | the agent is at work; steps every `timing-spinner` |
| ▲ | `mu-mark--waiting` | `waiting` | it needs the person: a question, a permission, a look |
| ✓ | `mu-mark--done` | `done` | its turn finished |
| ✗ | `mu-mark--error` | `waiting` | its program ended with an error |
| ○ | `mu-mark--ended` | `still` | not started, or ended |

The not-seen dot (`mu-dot`, `size-dot`, `news`) sits after the age, never in place of the mark.

**The consumer provides** the status and, beside the mark, the words for it ("wants to run cargo build", "finished"). A mark never stands alone: its row's second line or its tooltip says the same in words.

- Under *Reduce motion* the spinner holds one frame.
