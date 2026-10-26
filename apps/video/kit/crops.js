// Written by record.mjs: the kit's shots, and every crop and click point
// on them, measured from the page, in window pixels (the PNGs are twice that).
export const SCALE = 2;
export const shots = {
  "rows": {
    "file": "rows.png",
    "width": 1440,
    "height": 1240
  },
  "gemini": {
    "file": "gemini.png",
    "width": 1440,
    "height": 1240
  },
  "list": {
    "file": "list.png",
    "width": 1440,
    "height": 900
  },
  "start-empty": {
    "file": "start-empty.png",
    "width": 1440,
    "height": 900
  },
  "start": {
    "file": "start.png",
    "width": 1440,
    "height": 900
  },
  "started": {
    "file": "started.png",
    "width": 1440,
    "height": 900
  },
  "asking": {
    "file": "asking.png",
    "width": 1440,
    "height": 900
  },
  "question": {
    "file": "question.png",
    "width": 1440,
    "height": 900
  },
  "answered": {
    "file": "answered.png",
    "width": 1440,
    "height": 900
  },
  "finished": {
    "file": "finished.png",
    "width": 1440,
    "height": 900
  },
  "turns": {
    "file": "turns.png",
    "width": 1440,
    "height": 900
  },
  "turn-diff": {
    "file": "turn-diff.png",
    "width": 1440,
    "height": 900
  },
  "put-back": {
    "file": "put-back.png",
    "width": 1440,
    "height": 900
  },
  "put-back-done": {
    "file": "put-back-done.png",
    "width": 1440,
    "height": 900
  },
  "tickets": {
    "file": "tickets.png",
    "width": 1440,
    "height": 900
  },
  "assign": {
    "file": "assign.png",
    "width": 1440,
    "height": 900
  },
  "assigned": {
    "file": "assigned.png",
    "width": 1440,
    "height": 900
  },
  "project": {
    "file": "project.png",
    "width": 1440,
    "height": 900
  },
  "ssh": {
    "file": "ssh.png",
    "width": 1440,
    "height": 900
  }
};
export const crops = {
  "row-split": {
    "shot": "rows",
    "x": 44,
    "y": 288,
    "w": 235,
    "h": 48
  },
  "row-flaky": {
    "shot": "rows",
    "x": 44,
    "y": 418,
    "w": 235,
    "h": 48
  },
  "row-agp": {
    "shot": "rows",
    "x": 44,
    "y": 738,
    "w": 235,
    "h": 48
  },
  "row-gemini": {
    "shot": "rows",
    "x": 44,
    "y": 818,
    "w": 235,
    "h": 48
  },
  "badge-gemini": {
    "shot": "gemini",
    "x": 309,
    "y": 44,
    "w": 294.3125,
    "h": 34
  },
  "banner-gemini": {
    "shot": "gemini",
    "x": 309,
    "y": 44,
    "w": 1119,
    "h": 34
  },
  "sidebar": {
    "shot": "asking",
    "x": 0,
    "y": 0,
    "w": 288,
    "h": 872
  },
  "list-working": {
    "shot": "list",
    "x": 44,
    "y": 288,
    "w": 235,
    "h": 48
  },
  "list-finished": {
    "shot": "list",
    "x": 44,
    "y": 468,
    "w": 235,
    "h": 48
  },
  "list-needs-you": {
    "shot": "list",
    "x": 10,
    "y": 162,
    "w": 132.5,
    "h": 24
  },
  "start-box": {
    "shot": "start",
    "x": 464.5,
    "y": 116,
    "w": 800,
    "h": 156
  },
  "prompt": {
    "shot": "start",
    "x": 477.5,
    "y": 129,
    "w": 774,
    "h": 96
  },
  "row-new": {
    "shot": "started",
    "x": 44,
    "y": 628,
    "w": 235,
    "h": 48
  },
  "branch-new": {
    "shot": "started",
    "x": 28,
    "y": 602,
    "w": 251,
    "h": 24
  },
  "row-codex-asking": {
    "shot": "asking",
    "x": 44,
    "y": 548,
    "w": 235,
    "h": 48
  },
  "needs-you-tab": {
    "shot": "asking",
    "x": 10,
    "y": 162,
    "w": 132.5,
    "h": 24
  },
  "badge-codex": {
    "shot": "question",
    "x": 309,
    "y": 44,
    "w": 256.984375,
    "h": 34
  },
  "banner-codex": {
    "shot": "question",
    "x": 309,
    "y": 44,
    "w": 1119,
    "h": 34
  },
  "terminal-question": {
    "shot": "question",
    "x": 309,
    "y": 48,
    "w": 1123,
    "h": 806
  },
  "row-codex-answered": {
    "shot": "answered",
    "x": 44,
    "y": 548,
    "w": 235,
    "h": 48
  },
  "terminal-answered": {
    "shot": "answered",
    "x": 309,
    "y": 48,
    "w": 1123,
    "h": 806
  },
  "row-codex-finished": {
    "shot": "finished",
    "x": 44,
    "y": 548,
    "w": 235,
    "h": 48
  },
  "terminal-finished": {
    "shot": "finished",
    "x": 309,
    "y": 48,
    "w": 1123,
    "h": 806
  },
  "overlap": {
    "shot": "turns",
    "x": 343,
    "y": 92,
    "w": 1065.5,
    "h": 24
  },
  "turns-before": {
    "shot": "turns",
    "x": 309,
    "y": 136,
    "w": 280,
    "h": 198
  },
  "header-before": {
    "shot": "turns",
    "x": 337,
    "y": 12,
    "w": 548.5,
    "h": 20
  },
  "diff": {
    "shot": "turn-diff",
    "x": 602,
    "y": 170,
    "w": 817,
    "h": 685
  },
  "dialog-put-back": {
    "shot": "put-back",
    "x": 500,
    "y": 345,
    "w": 440,
    "h": 210
  },
  "turns-after": {
    "shot": "put-back-done",
    "x": 309,
    "y": 136,
    "w": 280,
    "h": 248
  },
  "header-after": {
    "shot": "put-back-done",
    "x": 337,
    "y": 12,
    "w": 541,
    "h": 20
  },
  "put-back-line": {
    "shot": "put-back-done",
    "x": 309,
    "y": 136,
    "w": 280,
    "h": 48
  },
  "ticket-list": {
    "shot": "tickets",
    "x": 297,
    "y": 16,
    "w": 403,
    "h": 532
  },
  "ticket-detail": {
    "shot": "tickets",
    "x": 729,
    "y": 16,
    "w": 691,
    "h": 844
  },
  "ticket-line": {
    "shot": "tickets",
    "x": 297,
    "y": 112,
    "w": 403,
    "h": 32
  },
  "dialog-assign": {
    "shot": "assign",
    "x": 500,
    "y": 182,
    "w": 440,
    "h": 536
  },
  "row-ticket": {
    "shot": "assigned",
    "x": 44,
    "y": 708,
    "w": 235,
    "h": 48
  },
  "branch-ticket": {
    "shot": "assigned",
    "x": 28,
    "y": 682,
    "w": 251,
    "h": 24
  },
  "pr-approved": {
    "shot": "project",
    "x": 464.5,
    "y": 460,
    "w": 800,
    "h": 28
  },
  "pr-draft": {
    "shot": "project",
    "x": 464.5,
    "y": 579,
    "w": 800,
    "h": 28
  },
  "where": {
    "shot": "project",
    "x": 464.5,
    "y": 304,
    "w": 800,
    "h": 517
  },
  "terminal-ssh": {
    "shot": "ssh",
    "x": 309,
    "y": 48,
    "w": 389.5,
    "h": 130
  }
};
export const points = {
  "new-session": {
    "shot": "list",
    "x": 144,
    "y": 62
  },
  "start": {
    "shot": "start",
    "x": 1169,
    "y": 247
  },
  "codex": {
    "shot": "asking",
    "x": 174,
    "y": 563
  },
  "changes": {
    "shot": "finished",
    "x": 1384,
    "y": 22
  },
  "turn": {
    "shot": "turns",
    "x": 449,
    "y": 160
  },
  "put-back": {
    "shot": "turn-diff",
    "x": 648,
    "y": 58
  },
  "put-back-files": {
    "shot": "put-back",
    "x": 852,
    "y": 515
  },
  "assign": {
    "shot": "tickets",
    "x": 1389,
    "y": 846
  },
  "start-assigned": {
    "shot": "assign",
    "x": 848,
    "y": 678
  }
};
