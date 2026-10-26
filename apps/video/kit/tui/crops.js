// Written by record.mjs: the terminal UI's shots, and every crop on them,
// measured from the terminal's grid, in its pixels (the PNGs are twice that).
export const SCALE = 2;
export const shots = {
  "home": {
    "file": "home.png",
    "width": 1438,
    "height": 882
  },
  "list": {
    "file": "list.png",
    "width": 1438,
    "height": 882
  },
  "worktree": {
    "file": "worktree.png",
    "width": 1438,
    "height": 882
  },
  "worktree-typed": {
    "file": "worktree-typed.png",
    "width": 1438,
    "height": 882
  },
  "worktree-made": {
    "file": "worktree-made.png",
    "width": 1438,
    "height": 882
  },
  "new-session": {
    "file": "new-session.png",
    "width": 1438,
    "height": 882
  },
  "new-typed": {
    "file": "new-typed.png",
    "width": 1438,
    "height": 882
  },
  "started": {
    "file": "started.png",
    "width": 1438,
    "height": 882
  },
  "asking": {
    "file": "asking.png",
    "width": 1438,
    "height": 882
  },
  "queue": {
    "file": "queue.png",
    "width": 1438,
    "height": 882
  },
  "answering": {
    "file": "answering.png",
    "width": 1438,
    "height": 882
  },
  "answered": {
    "file": "answered.png",
    "width": 1438,
    "height": 882
  },
  "changes": {
    "file": "changes.png",
    "width": 1438,
    "height": 882
  },
  "turn-diff": {
    "file": "turn-diff.png",
    "width": 1438,
    "height": 882
  },
  "put-back": {
    "file": "put-back.png",
    "width": 1438,
    "height": 882
  },
  "put-back-done": {
    "file": "put-back-done.png",
    "width": 1438,
    "height": 882
  },
  "tickets": {
    "file": "tickets.png",
    "width": 1438,
    "height": 882
  },
  "assign": {
    "file": "assign.png",
    "width": 1438,
    "height": 882
  },
  "assigned": {
    "file": "assigned.png",
    "width": 1438,
    "height": 882
  }
};
export const crops = {
  "row-split": {
    "shot": "list",
    "x": 10,
    "y": 147,
    "w": 347,
    "h": 49
  },
  "row-flaky": {
    "shot": "list",
    "x": 10,
    "y": 220.5,
    "w": 347,
    "h": 49
  },
  "row-agp": {
    "shot": "list",
    "x": 10,
    "y": 343,
    "w": 347,
    "h": 49
  },
  "list-frame": {
    "shot": "list",
    "x": 0,
    "y": 24.5,
    "w": 367,
    "h": 833
  },
  "preview": {
    "shot": "list",
    "x": 367,
    "y": 24.5,
    "w": 1071,
    "h": 833
  },
  "counts": {
    "shot": "list",
    "x": 693.5,
    "y": 0,
    "w": 194,
    "h": 24.5
  },
  "worktree-dialog": {
    "shot": "worktree-typed",
    "x": 347,
    "y": 367.5,
    "w": 734.5,
    "h": 147
  },
  "worktree-text": {
    "shot": "worktree-typed",
    "x": 357,
    "y": 416.5,
    "w": 102,
    "h": 24.5
  },
  "worktree-line": {
    "shot": "worktree-made",
    "x": 10,
    "y": 269.5,
    "w": 347,
    "h": 24.5
  },
  "session-dialog": {
    "shot": "new-typed",
    "x": 347,
    "y": 343,
    "w": 734.5,
    "h": 196
  },
  "session-text": {
    "shot": "new-typed",
    "x": 357,
    "y": 441,
    "w": 285.5,
    "h": 24.5
  },
  "row-new": {
    "shot": "started",
    "x": 10,
    "y": 294,
    "w": 347,
    "h": 49
  },
  "new-preview": {
    "shot": "started",
    "x": 367,
    "y": 24.5,
    "w": 1071,
    "h": 833
  },
  "row-asking": {
    "shot": "asking",
    "x": 10,
    "y": 147,
    "w": 347,
    "h": 49
  },
  "waiting": {
    "shot": "asking",
    "x": 826,
    "y": 0,
    "w": 92,
    "h": 24.5
  },
  "needs-you-tab": {
    "shot": "asking",
    "x": 285.5,
    "y": 0,
    "w": 112,
    "h": 24.5
  },
  "queue": {
    "shot": "queue",
    "x": 0,
    "y": 24.5,
    "w": 367,
    "h": 833
  },
  "question": {
    "shot": "answering",
    "x": 397.5,
    "y": 220.5,
    "w": 530.5,
    "h": 171.5
  },
  "allowed": {
    "shot": "answered",
    "x": 397.5,
    "y": 392,
    "w": 530.5,
    "h": 122.5
  },
  "row-answered": {
    "shot": "answered",
    "x": 10,
    "y": 147,
    "w": 347,
    "h": 49
  },
  "changes": {
    "shot": "changes",
    "x": 826,
    "y": 24.5,
    "w": 612,
    "h": 833
  },
  "turn": {
    "shot": "changes",
    "x": 867,
    "y": 171.5,
    "w": 448.5,
    "h": 24.5
  },
  "diff": {
    "shot": "turn-diff",
    "x": 826,
    "y": 24.5,
    "w": 612,
    "h": 833
  },
  "put-back-dialog": {
    "shot": "put-back",
    "x": 347,
    "y": 343,
    "w": 734.5,
    "h": 196
  },
  "changes-after": {
    "shot": "put-back-done",
    "x": 826,
    "y": 24.5,
    "w": 612,
    "h": 833
  },
  "put-back-line": {
    "shot": "put-back-done",
    "x": 867,
    "y": 98,
    "w": 448.5,
    "h": 24.5
  },
  "ticket-list": {
    "shot": "tickets",
    "x": 0,
    "y": 24.5,
    "w": 428.5,
    "h": 833
  },
  "ticket": {
    "shot": "tickets",
    "x": 428.5,
    "y": 24.5,
    "w": 1009.5,
    "h": 833
  },
  "ticket-line": {
    "shot": "tickets",
    "x": 41,
    "y": 73.5,
    "w": 306,
    "h": 24.5
  },
  "assign-dialog": {
    "shot": "assign",
    "x": 347,
    "y": 318.5,
    "w": 734.5,
    "h": 220.5
  },
  "ticket-agent": {
    "shot": "assigned",
    "x": 367,
    "y": 24.5,
    "w": 1071,
    "h": 833
  },
  "ticket-mark": {
    "shot": "assigned",
    "x": 10,
    "y": 73.5,
    "w": 347,
    "h": 24.5
  }
};
export const points = {};
