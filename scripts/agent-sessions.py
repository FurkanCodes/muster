"""Where agents' time goes, measured from this checkout's Claude Code transcripts.

Run from the repository's root, to compare with an earlier run before and
after changing CLAUDE.md:

    python3 -I scripts/agent-sessions.py [out.json] [session id to skip]

Reads every session of this checkout in ~/.claude/projects (its subagents'
too), streamed, and writes the measurements as JSON (default
agent-sessions.json): time by tool and command, whole-suite test runs,
reruns, failures, exploration, polling, subagents and tokens. The JSON
quotes commands and the person's corrections, so it stays local.
"""
import collections
import glob
import json
import os
import re
import shlex
import sys
from datetime import datetime

REPO = os.getcwd()
ROOT = os.path.join(os.path.expanduser("~/.claude/projects"), REPO.replace("/", "-").replace(".", "-"))
OUT = sys.argv[1] if len(sys.argv) > 1 else "agent-sessions.json"
SKIP = sys.argv[2] if len(sys.argv) > 2 else None


def ts(s):
    if not s:
        return None
    try:
        return datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp()
    except Exception:
        return None


def text_of(c):
    if c is None:
        return ""
    if isinstance(c, str):
        return c
    if isinstance(c, list):
        out = []
        for b in c:
            if isinstance(b, dict):
                if b.get("type") == "text":
                    out.append(b.get("text", ""))
                elif b.get("type") == "tool_result":
                    out.append(text_of(b.get("content")))
            elif isinstance(b, str):
                out.append(b)
        return "\n".join(out)
    return str(c)


# ---------------- Bash command classification ----------------
PREFIX_RE = re.compile(r"^\s*(?:rtk\s+(?:proxy\s+)?|time\s+|timeout\s+\d+[sm]?\s+|[A-Z_][A-Z0-9_]*=\S*\s+)+")
FLAG_WITH_VALUE = {"-p", "--package", "--features", "-F", "-j", "--jobs", "--profile", "--manifest-path",
                   "--target", "--test-threads", "--color", "--message-format", "--retries", "-E",
                   "--filterset", "--partition", "--target-dir", "--config", "--exclude", "--bin",
                   "--test", "--lib-name", "--run-ignored", "--status-level", "--final-status-level",
                   "--failure-output", "--success-output", "--cargo-profile", "-Z", "--archive-file",
                   "--workspace-remap", "--tool-config-file", "--no-tests", "--max-fail"}


def segments(cmd):
    return [s.strip() for s in re.split(r"&&|\|\||;|\n|\|", cmd) if s.strip()]


def strip_prefix(seg):
    seg = re.sub(r"^\(?\s*cd\s+\S+\s*$", "", seg)
    return PREFIX_RE.sub("", seg).strip()


def cargo_test_detail(seg, cmd):
    try:
        toks = shlex.split(seg)
    except Exception:
        toks = seg.split()
    tauri = "src-tauri" in cmd or "src-tauri" in seg
    scoped = any(t in ("-p", "--package") or t.startswith("--package=") or t.startswith("-p") and len(t) > 2 and not t.startswith("--") for t in toks)
    workspace_flag = "--workspace" in toks or "--all" in toks
    filt = False
    if "-E" in toks or "--test" in toks or "--lib" in toks or "--bin" in toks or "--doc" in toks:
        filt = True
    # positional filters after the subcommand
    try:
        i = toks.index("nextest") + 2 if "nextest" in toks else toks.index("test") + 1
    except ValueError:
        i = len(toks)
    skip = False
    after_dd = False
    for t in toks[i:]:
        if t == "--":
            after_dd = True
            continue
        if skip:
            skip = False
            continue
        if t in FLAG_WITH_VALUE and not after_dd:
            skip = True
            continue
        if t.startswith("-"):
            continue
        if t in (">", ">>", "2>", "&>", "1>"):
            skip = True
            continue
        if ">" in t or t.startswith(("/", "$", "<")):
            continue
        filt = True
    if tauri:
        scope = "tauri-workspace"
    elif scoped and not workspace_flag:
        scope = "scoped(-p)"
    else:
        scope = "workspace-wide"
    return scope, filt


def classify_bash(cmd):
    """Return (category, detail) for the heaviest thing in a compound command."""
    segs = [strip_prefix(s) for s in segments(cmd)]
    found = []
    for s in segs:
        low = s
        m = re.match(r"(?:\S*/)?cargo\s+(\+\S+\s+)?(\S+)(?:\s+(\S+))?", low)
        if m:
            sub = m.group(2)
            if (sub == "nextest" or sub == "test") and "--version" in s:
                found.append((50, "cargo other", "version"))
            elif sub == "nextest" or sub == "test":
                scope, filt = cargo_test_detail(s, cmd)
                found.append((100, "cargo test/nextest", f"{scope}{' filtered' if filt else ''}"))
            elif sub == "clippy":
                found.append((80, "cargo clippy", "tauri" if "src-tauri" in cmd else ""))
            elif sub in ("build", "b"):
                found.append((85, "cargo build", "release" if "--release" in s else ""))
            elif sub in ("check", "c"):
                found.append((75, "cargo check", ""))
            elif sub == "fmt":
                found.append((30, "cargo fmt", ""))
            elif sub == "run":
                found.append((70, "cargo run", ""))
            elif sub in ("doc", "shear", "deny", "dist", "install", "update", "tree", "metadata", "llvm-cov", "insta"):
                found.append((60, "cargo other", sub))
            else:
                found.append((50, "cargo other", sub))
            continue
        m = re.match(r"(?:pnpm|npm|npx|yarn)\s+(?:--dir\s+\S+\s+|-C\s+\S+\s+|--filter\s+\S+\s+)*(?:run\s+)?(\S+)(?:\s+(\S+))?", low)
        if m:
            sub, sub2 = m.group(1), m.group(2) or ""
            if sub == "e2e:app":
                found.append((98, "pnpm e2e:app", ""))
            elif sub == "e2e" or sub == "playwright":
                found.append((95, "pnpm e2e", ""))
            elif sub == "tauri" and sub2 == "build":
                found.append((97, "pnpm tauri build", ""))
            elif sub == "tauri":
                found.append((65, "pnpm tauri other", sub2))
            elif sub in ("test", "vitest"):
                found.append((90, "pnpm test", ""))
            elif sub in ("lint", "typecheck", "tsc", "biome", "format"):
                found.append((40, "pnpm lint/typecheck", sub))
            elif sub in ("install", "i", "add"):
                found.append((45, "pnpm install", ""))
            elif sub in ("build", "build:mock"):
                found.append((70, "pnpm build", sub))
            elif sub == "bindings":
                found.append((70, "pnpm bindings", ""))
            else:
                found.append((35, "pnpm other", sub))
            continue
        if re.match(r"(?:\S*/)?(wdio|playwright)\b", low):
            found.append((95, "pnpm e2e", "direct"))
            continue
        m = re.match(r"git\s+(?:-C\s+\S+\s+)*(\S+)", low)
        if m:
            found.append((20, "git", m.group(1)))
            continue
        m = re.match(r"gh\s+(\S+)(?:\s+(\S+))?", low)
        if m:
            found.append((25, "gh", f"{m.group(1)} {m.group(2) or ''}".strip()))
            continue
        if re.match(r"docker\b", low):
            found.append((55, "docker", ""))
            continue
        if re.match(r"(?:\S*/)?(target/\S+/)?muster(d|-\w+)?\b", low) or re.match(r"\S*target/(debug|release)/", low):
            found.append((15, "muster binary", ""))
            continue
        if re.match(r"(?:\S*/)?codegraph\b", low):
            found.append((12, "codegraph (shell)", ""))
            continue
        if re.match(r"(cat|head|tail|sed|awk|less|wc|grep|rg|find|ls|tree|fd|bat|read)\b", low):
            found.append((5, "shell explore (cat/grep/find/ls/sed)", low.split()[0]))
            continue
        if re.match(r"(sleep|wait)\b", low) or re.match(r"(until|while)\b", low):
            found.append((10, "sleep/wait/poll", ""))
            continue
        if re.match(r"python3?\b", low):
            found.append((8, "python", ""))
            continue
        if re.match(r"(curl|wget)\b", low):
            found.append((8, "curl", ""))
            continue
        if re.match(r"(brew|osascript|open|killall|pkill|kill|ps|lsof|pgrep|tmux|screencapture|xcrun|codesign|security|defaults)\b", low):
            found.append((9, "system (brew/ps/kill/osascript…)", low.split()[0]))
            continue
        if re.match(r"(\./)?scripts/", low) or re.match(r"(bash|sh|zsh)\s", low):
            found.append((9, "scripts/shell", low.split()[0]))
            continue
    if not found:
        return "other", ""
    found.sort(key=lambda x: -x[0])
    return found[0][1], found[0][2]


def norm_cmd(cmd):
    c = cmd.strip()
    c = re.sub(r"\s*2>&1", "", c)
    c = re.sub(r"\s*\|\s*(tail|head|grep|rg)\b[^|&;]*", "", c)
    c = re.sub(r"\brtk\s+", "", c)
    c = re.sub(r"\s+", " ", c)
    return c.strip()


# ---------------- error classification ----------------
def classify_error(name, text, cat):
    t = text[:6000]
    tl = t.lower()
    if "denied by the claude code auto mode classifier" in tl:
        return "permission: auto-mode classifier denied"
    if "doesn't want to proceed" in tl or "user rejected" in tl or "the user doesn't want" in tl or "permission to use" in tl and "denied" in tl:
        return "permission: user rejected tool call"
    if "[request interrupted" in tl or "interrupted by user" in tl:
        return "user interrupted"
    if "hook" in tl and ("blocked" in tl or "pretooluse" in tl or "posttooluse" in tl or "hook error" in tl):
        return "hook blocked/error"
    if name in ("Edit", "MultiEdit", "Write", "NotebookEdit"):
        if "string to replace not found" in tl:
            return "Edit: old_string not found"
        if "has not been read yet" in tl or "read it first" in tl:
            return "Edit/Write: file not read first"
        if "modified since read" in tl or "has been modified" in tl:
            return "Edit/Write: file changed since read"
        if "matches of the string" in tl or "found" in tl and "matches" in tl:
            return "Edit: old_string not unique"
        return "Edit/Write: other"
    if name == "Read":
        if "does not exist" in tl or "no such file" in tl:
            return "Read: file missing"
        if "exceeds maximum" in tl or "too large" in tl or "exceeds" in tl:
            return "Read: file too large"
        return "Read: other"
    if "inputvalidationerror" in tl or "invalid_type" in tl or "unexpected parameter" in tl:
        return "tool input validation"
    if "timed out" in tl or "timeout" in tl and "exit code 124" in tl:
        return "timeout"
    if name == "Bash":
        if re.search(r"error\[e\d{4}\]|could not compile|error: could not compile|aborting due to", tl):
            if cat == "cargo clippy" and "-d warnings" not in tl and re.search(r"error: (this|used|unneeded|redundant|you|the|item|variables|casting|called|length|manual|very complex|large|function)", tl):
                return "clippy lint failure"
            return "Rust compile error"
        if cat == "cargo clippy" or "clippy::" in tl:
            return "clippy lint failure"
        if cat == "cargo test/nextest" and re.search(r"test result: failed|failed|panicked|\bfail\b", tl):
            return "Rust test failure"
        if cat in ("pnpm test", "pnpm e2e", "pnpm e2e:app") and re.search(r"fail|✗|×|error", tl):
            return "JS/e2e test failure"
        if cat == "pnpm lint/typecheck":
            return "lint/typecheck failure (JS)"
        if cat == "cargo fmt":
            return "cargo fmt diff"
        if "command not found" in tl or "no such file or directory" in tl:
            return "Bash: command/file not found"
        if cat in ("git",):
            return "git error"
        if cat == "gh":
            return "gh error"
        if cat == "shell explore (cat/grep/find/ls/sed)":
            return "shell explore nonzero (e.g. grep no match)"
        return f"Bash nonzero exit: {cat}"
    if name.startswith("mcp__"):
        return "MCP tool error"
    return f"{name}: other"


CORRECTION_RE = re.compile(
    r"^\s*(no\b|nope|don'?t|do not|stop|wait\b|actually|that'?s (wrong|not)|not what|why (did|do|are) you|"
    r"you (should|shouldn'?t|didn'?t|forgot|missed)|instead\b|wrong|undo|revert|never\b|please don'?t|i said|i asked)",
    re.I)


def is_human_text(o):
    if o.get("type") != "user" or o.get("isMeta") or o.get("isCompactSummary"):
        return None
    c = o.get("message", {}).get("content")
    if isinstance(c, list):
        if any(isinstance(b, dict) and b.get("type") == "tool_result" for b in c):
            return None
        c = text_of(c)
    if not isinstance(c, str):
        return None
    s = c.strip()
    if not s:
        return None
    if s.startswith(("<command-", "<local-command", "<task-notification", "<system-reminder", "Caveat:",
                     "This session is being continued", "<bash-", "<user-memory")):
        return None
    return s


def analyze_file(path, sid, kind):
    uses = {}  # id -> dict
    results = {}
    notif = {}
    usage_by_msg = {}
    compacts = []
    human = []
    turn_ms = 0
    cost_state = None
    first_ts = last_ts = None
    order = []
    with open(path, "r", errors="replace") as f:
        for line in f:
            try:
                o = json.loads(line)
            except Exception:
                continue
            t = o.get("type")
            tt = ts(o.get("timestamp"))
            if tt and t in ("user", "assistant"):
                first_ts = tt if first_ts is None else min(first_ts, tt)
                last_ts = tt if last_ts is None else max(last_ts, tt)
            if t == "assistant":
                m = o.get("message", {})
                mid = m.get("id") or o.get("uuid")
                if m.get("usage"):
                    usage_by_msg[mid] = m["usage"]
                for b in m.get("content") or []:
                    if isinstance(b, dict) and b.get("type") == "tool_use":
                        uses[b["id"]] = {"name": b.get("name"), "input": b.get("input") or {}, "t": tt, "mid": mid}
                        order.append(b["id"])
            elif t == "user":
                c = o.get("message", {}).get("content")
                if isinstance(c, list):
                    for b in c:
                        if isinstance(b, dict) and b.get("type") == "tool_result":
                            tur = o.get("toolUseResult")
                            results[b.get("tool_use_id")] = {
                                "t": tt, "is_error": bool(b.get("is_error")),
                                "text": text_of(b.get("content"))[:8000],
                                "len": len(text_of(b.get("content"))),
                                "tur": {k: tur.get(k) for k in ("interrupted", "backgroundTaskId", "timedOutAfterMs", "totalDurationMs", "totalTokens", "totalToolUseCount", "status")} if isinstance(tur, dict) else {},
                            }
                h = is_human_text(o)
                if h is not None:
                    human.append({"t": tt, "text": h})
            elif t == "queue-operation":
                c = o.get("content")
                if isinstance(c, str) and "<task-notification>" in c:
                    m = re.search(r"<tool-use-id>(\S+?)</tool-use-id>", c)
                    st = re.search(r"<status>(\w+)</status>", c)
                    ex = re.search(r"exit code (\d+)", c)
                    if m and m.group(1) not in notif:
                        notif[m.group(1)] = {"t": tt, "status": st.group(1) if st else None,
                                             "exit": int(ex.group(1)) if ex else None}
            elif t == "system":
                st = o.get("subtype")
                if st == "compact_boundary":
                    cm = o.get("compactMetadata") or {}
                    compacts.append({"trigger": cm.get("trigger"), "preTokens": cm.get("preTokens"), "durationMs": cm.get("durationMs")})
                elif st == "turn_duration":
                    turn_ms += o.get("durationMs") or 0
            elif t == "cost-state":
                cost_state = o
    # exclusive time: calls issued in one assistant message overlap; charge each only for
    # the time after the previous result of the same message arrived
    excl = {}
    bymsg = collections.defaultdict(list)
    for uid in order:
        if uid in results and results[uid]["t"] and uses[uid]["t"]:
            bymsg[uses[uid]["mid"]].append(uid)
    for mid_, ids in bymsg.items():
        ids.sort(key=lambda i: results[i]["t"])
        prev = None
        for i in ids:
            start = uses[i]["t"] if prev is None else max(uses[i]["t"], prev)
            excl[i] = max(0.0, results[i]["t"] - start)
            prev = results[i]["t"]
    calls = []
    for uid in order:
        u = uses[uid]
        r = results.get(uid)
        rec = {"id": uid, "name": u["name"], "t0": u["t"], "session": sid, "kind": kind}
        inp = u["input"]
        if u["name"] == "Bash":
            cmd = inp.get("command", "")
            rec["cmd"] = cmd
            rec["cat"], rec["detail"] = classify_bash(cmd)
            rec["bg_requested"] = bool(inp.get("run_in_background"))
        elif u["name"] in ("Read",):
            rec["path"] = inp.get("file_path")
            rec["partial"] = bool(inp.get("offset") or inp.get("limit"))
        elif u["name"] in ("Edit", "Write", "MultiEdit"):
            rec["path"] = inp.get("file_path")
        elif u["name"] in ("Agent", "Task"):
            rec["desc"] = inp.get("description")
            rec["subtype"] = inp.get("subagent_type")
            rec["bg_requested"] = bool(inp.get("run_in_background"))
        if r:
            rec["dur_raw"] = (r["t"] - u["t"]) if (r["t"] and u["t"]) else None
            rec["dur"] = excl.get(uid, rec["dur_raw"])
            rec["is_error"] = r["is_error"]
            rec["len"] = r["len"]
            rec["interrupted"] = bool(r["tur"].get("interrupted"))
            rec["bg_id"] = r["tur"].get("backgroundTaskId")
            rec["auto_bg_timeout"] = r["tur"].get("timedOutAfterMs")
            rec["text_head"] = r["text"][:300]
            if u["name"] == "Bash":
                rec["text_head_full"] = r["text"]
            if r["is_error"]:
                rec["err"] = classify_error(u["name"], r["text"], rec.get("cat"))
            elif u["name"] == "Bash" and re.search(r"^Error: Exit code [1-9]", r["text"]):
                rec["is_error"] = True
                rec["err"] = classify_error("Bash", r["text"], rec.get("cat"))
        else:
            rec["dur"] = None
        n = notif.get(uid)
        if n and n["t"] and u["t"]:
            rec["bg_dur"] = n["t"] - u["t"]
            rec["bg_exit"] = n["exit"]
            rec["bg_status"] = n["status"]
        calls.append(rec)
    usage = collections.Counter()
    for us in usage_by_msg.values():
        for k in ("input_tokens", "output_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"):
            usage[k] += us.get(k) or 0
    return {
        "calls": calls, "human": human, "compacts": compacts, "turn_ms": turn_ms,
        "usage": dict(usage), "n_assistant_msgs": len(usage_by_msg),
        "span_s": (last_ts - first_ts) if first_ts else 0,
        "cost": {k: cost_state.get(k) for k in ("totalCostUSD", "totalAPIDuration", "totalToolDuration", "totalDuration")} if cost_state else None,
    }


def main():
    sessions = {}
    subs = {}
    for p in sorted(glob.glob(os.path.join(ROOT, "*.jsonl"))):
        sid = os.path.basename(p)[:-6]
        if sid == SKIP:
            continue
        sessions[sid] = analyze_file(p, sid, "main")
    for p in sorted(glob.glob(os.path.join(ROOT, "*", "subagents", "*.jsonl"))):
        parent = p.split(os.sep)[-3]
        if parent == SKIP:
            continue
        aid = os.path.basename(p)[:-6]
        meta = {}
        mp = p[:-6] + ".meta.json"
        if os.path.exists(mp):
            try:
                meta = json.load(open(mp))
            except Exception:
                pass
        r = analyze_file(p, parent, "sub")
        r["meta"] = meta
        subs[f"{parent}/{aid}"] = r

    allcalls = [c for s in sessions.values() for c in s["calls"]]
    subcalls = [c for s in subs.values() for c in s["calls"]]
    res = {}

    def agg(calls):
        by_tool = collections.defaultdict(lambda: [0, 0.0])
        by_cat = collections.defaultdict(lambda: [0, 0.0, 0.0, 0])  # n, fg s, bg s, n_err
        for c in calls:
            d = c.get("dur") or 0
            by_tool[c["name"]][0] += 1
            by_tool[c["name"]][1] += d
            if c["name"] == "Bash":
                k = c["cat"] + (f" [{c['detail']}]" if c["cat"] == "cargo test/nextest" else "")
                by_cat[k][0] += 1
                by_cat[k][1] += d
                by_cat[k][2] += c.get("bg_dur") or 0
                by_cat[k][3] += 1 if c.get("is_error") else 0
        return ({k: {"n": v[0], "s": round(v[1], 1)} for k, v in sorted(by_tool.items(), key=lambda x: -x[1][1])},
                {k: {"n": v[0], "fg_s": round(v[1], 1), "bg_s": round(v[2], 1), "err": v[3]} for k, v in sorted(by_cat.items(), key=lambda x: -(x[1][1] + x[1][2]))})

    res["main_by_tool"], res["main_bash_by_cat"] = agg(allcalls)
    res["sub_by_tool"], res["sub_bash_by_cat"] = agg(subcalls)

    both = allcalls + subcalls
    bash = [c for c in both if c["name"] == "Bash"]
    eff = lambda c: max(c.get("dur") or 0, c.get("bg_dur") or 0)
    res["top_slowest"] = [
        {"s": round(eff(c)), "fg_s": round(c.get("dur") or 0), "bg": bool(c.get("bg_dur")), "cat": c["cat"], "detail": c["detail"],
         "kind": c["kind"], "session": c["session"][:8], "err": c.get("is_error"), "cmd": c["cmd"][:160]}
        for c in sorted(bash, key=lambda c: -eff(c))[:25]]

    # full workspace suite
    ws = [c for c in bash if c["cat"] == "cargo test/nextest" and c["detail"].startswith("workspace-wide")]
    ws_unf = [c for c in ws if "filtered" not in c["detail"]]
    res["workspace_tests"] = {
        "n_any": len(ws), "s_any": round(sum(eff(c) for c in ws)),
        "n_unfiltered": len(ws_unf), "s_unfiltered": round(sum(eff(c) for c in ws_unf)),
        "unfiltered_errors": sum(1 for c in ws_unf if c.get("is_error") or (c.get("bg_exit") or 0) != 0),
        "per_session": collections.Counter(c["session"][:8] for c in ws_unf),
        "examples": [c["cmd"][:120] for c in ws_unf[:8]],
        "all_cargo_segments": [next((sg[:110] for sg in segments(c["cmd"]) if re.search(r"cargo\s+(\+\S+\s+)?(test|nextest)", sg)), "")+"  ||cd:"+str(bool(re.search(r"cd\s+\S*crates/", c["cmd"]))) + f" s={round(eff(c))}" for c in ws],
    }
    tests_all = [c for c in bash if c["cat"] == "cargo test/nextest"]
    res["cargo_test_scope"] = {}
    for c in tests_all:
        d = res["cargo_test_scope"].setdefault(c["detail"], {"n": 0, "s": 0.0})
        d["n"] += 1
        d["s"] += eff(c)

    # repeated work per session (main + sub transcripts separately)
    rep = {"exact_reruns": 0, "exact_rerun_s": 0.0, "heavy_exact_reruns": 0, "heavy_exact_rerun_s": 0.0,
           "after_fail_reruns": 0, "after_fail_failed_s": 0.0, "top_repeated": []}
    heavy = {"cargo test/nextest", "cargo clippy", "cargo build", "cargo check", "pnpm e2e", "pnpm e2e:app",
             "pnpm test", "pnpm tauri build", "pnpm lint/typecheck"}
    groups = collections.defaultdict(list)
    for c in bash:
        groups[(c["session"], c["kind"])].append(c)
    toprep = collections.Counter()
    toprep_s = collections.Counter()
    fail_chain = collections.Counter()
    for (sid, kind), cs in groups.items():
        seen = collections.Counter()
        last_by_cat = {}
        for c in cs:
            n = norm_cmd(c["cmd"])
            if seen[n] > 0:
                rep["exact_reruns"] += 1
                rep["exact_rerun_s"] += eff(c)
                if c["cat"] in heavy:
                    rep["heavy_exact_reruns"] += 1
                    rep["heavy_exact_rerun_s"] += eff(c)
                    toprep[(sid[:8], n[:110])] += 1
                    toprep_s[(sid[:8], n[:110])] += eff(c)
            seen[n] += 1
            if c["cat"] in heavy:
                prev = last_by_cat.get(c["cat"])
                failed = c.get("is_error") or (c.get("bg_exit") not in (None, 0)) or bool(re.search(r"test result: FAILED|^\s*FAIL \[|panicked at|error\[E\d{4}\]|could not compile|\b[1-9]\d* failed", c.get("text_head_full") or "", re.M))
                if prev is not None and prev["failed"]:
                    rep["after_fail_reruns"] += 1
                    rep["after_fail_failed_s"] += prev["s"]
                    fail_chain[c["cat"]] += 1
                last_by_cat[c["cat"]] = {"failed": failed, "s": eff(c)}
    rep["top_repeated"] = [{"session": k[0], "cmd": k[1], "reruns": v, "s": round(toprep_s[k])} for k, v in toprep.most_common(12)]
    rep["after_fail_by_cat"] = dict(fail_chain)
    rep["exact_rerun_s"] = round(rep["exact_rerun_s"])
    rep["heavy_exact_rerun_s"] = round(rep["heavy_exact_rerun_s"])
    rep["after_fail_failed_s"] = round(rep["after_fail_failed_s"])
    res["repeats"] = rep

    # errors
    errs = collections.defaultdict(lambda: {"n": 0, "s": 0.0, "examples": []})
    for c in both:
        if c.get("is_error"):
            e = errs[c.get("err") or "unknown"]
            e["n"] += 1
            e["s"] += eff(c)
            if len(e["examples"]) < 3:
                e["examples"].append((c.get("cmd") or c.get("path") or c["name"])[:100] + " => " + (c.get("text_head") or "")[:140].replace("\n", " "))
    res["errors"] = {k: {"n": v["n"], "s": round(v["s"]), "examples": v["examples"]} for k, v in sorted(errs.items(), key=lambda x: -x[1]["n"])}
    res["errors_total"] = {"n": sum(v["n"] for v in errs.values()), "s": round(sum(v["s"] for v in errs.values())), "calls": len(both)}
    # content-based failures (exit codes are usually masked by pipes to tail/grep or '; echo $?')
    FAIL_RE = {
        "cargo test/nextest": re.compile(r"test result: FAILED|^\s*FAIL \[|panicked at|error\[E\d{4}\]|could not compile|\b[1-9]\d* failed|^failures:\s*$", re.M),
        "cargo clippy": re.compile(r"error\[E\d{4}\]|could not compile|^error: |clippy exit[ :=]*[1-9]|clippy=[1-9]", re.M),
        "cargo build": re.compile(r"error\[E\d{4}\]|could not compile", re.M),
        "cargo check": re.compile(r"error\[E\d{4}\]|could not compile", re.M),
        "pnpm test": re.compile(r"\bFAIL\b|failed", re.M),
        "pnpm e2e": re.compile(r"\bfailed\b|✘", re.M),
    }
    piped = collections.Counter(); masked = collections.Counter(); cf = collections.defaultdict(lambda: [0, 0.0]); compile_err = [0, 0.0]
    heavy_n = collections.Counter()
    for c in bash:
        if c["cat"] in FAIL_RE:
            heavy_n[c["cat"]] += 1
            if re.search(r"\|\s*(tail|head|grep|/usr/bin/grep|rg)|>\s*/dev/null|>\s*\S+\.(txt|log)|;\s*echo", c["cmd"]):
                piped[c["cat"]] += 1
            full = c.get("text_head_full") or ""
            if FAIL_RE[c["cat"]].search(full):
                cf[c["cat"]][0] += 1
                cf[c["cat"]][1] += eff(c)
                c["content_fail"] = True
                if not c.get("is_error"):
                    masked[c["cat"]] += 1
                if re.search(r"error\[E\d{4}\]|could not compile", full):
                    compile_err[0] += 1
                    compile_err[1] += eff(c)
    res["content_failures"] = {"by_cat": {k: {"n": v[0], "s": round(v[1]), "of_runs": heavy_n[k], "not_flagged_is_error": masked[k]} for k, v in cf.items()},
                               "runs_output_piped_or_redirected": dict(piped), "runs_by_cat": dict(heavy_n),
                               "rust_compile_error_runs": compile_err[0], "rust_compile_error_s": round(compile_err[1])}
    # compile time inside test/build commands, from cargo's 'Finished ... in X' line
    fin = collections.defaultdict(lambda: [0, 0.0, 0.0])
    for c in bash:
        if c["cat"].startswith("cargo"):
            m = re.findall(r"Finished `\w+` profile[^\n]* in (?:(\d+)m )?([\d.]+)s", c.get("text_head_full") or "")
            if m:
                ct = sum(int(a or 0) * 60 + float(b) for a, b in m)
                fin[c["cat"]][0] += 1
                fin[c["cat"]][1] += ct
                fin[c["cat"]][2] += c.get("dur") or 0
    res["cargo_compile_time_visible"] = {k: {"runs_with_finished_line": v[0], "compile_s": round(v[1]), "tool_s_of_those": round(v[2])} for k, v in fin.items()}
    # medians / p90 fg per category
    import statistics
    pc = collections.defaultdict(list)
    for c in bash:
        if c.get("dur") is not None:
            pc[c["cat"] + (" [" + c["detail"] + "]" if c["cat"] == "cargo test/nextest" else "")].append(c["dur"])
    res["fg_dist_by_cat"] = {k: {"n": len(v), "median": round(statistics.median(v), 1), "p90": round(sorted(v)[int(len(v) * .9)], 1), "max": round(max(v))} for k, v in pc.items() if len(v) >= 5}
    topfg = collections.defaultdict(list)
    for c in sorted(bash, key=lambda c: -(c.get("dur") or 0)):
        if len(topfg[c["cat"]]) < 4:
            topfg[c["cat"]].append({"fg_s": round(c.get("dur") or 0), "session": c["session"][:8], "kind": c["kind"], "cmd": c["cmd"][:150]})
    res["top_fg_by_cat"] = dict(topfg)
    res["top_slowest_fg"] = [{"fg_s": round(c.get("dur") or 0), "cat": c["cat"], "detail": c["detail"], "kind": c["kind"], "session": c["session"][:8], "cmd": c["cmd"][:150]} for c in sorted(bash, key=lambda c: -(c.get("dur") or 0))[:20]]
    for c in both:
        c.pop("text_head_full", None)
    import statistics as _st
    med = {}
    for name in ("Read", "Grep", "Glob", "Edit", "Write", "mcp__codegraph__codegraph_explore"):
        v = [c["dur"] for c in both if c["name"] == name and c.get("dur") is not None]
        if v:
            med[name] = {"n": len(v), "median_s": round(_st.median(v), 2), "sum_s": round(sum(v))}
    res["tool_latency"] = med
    polls = [c for c in bash if c["cat"] == "sleep/wait/poll" or re.search(r"\buntil\b|\bwhile\b.*sleep", c["cmd"])]
    unbounded = [c for c in polls if re.search(r"\buntil\b|\bwhile\b", c["cmd"]) and not re.search(r"-ge|-gt|-lt|-le|timeout|seq |for i in|\$n|\$i|SECONDS", c["cmd"])]
    res["polling"] = {"poll_cmds": len(polls), "poll_fg_s": round(sum(c.get("dur") or 0 for c in polls)), "poll_bg_s": round(sum(c.get("bg_dur") or 0 for c in polls)),
                      "unbounded_loops": len(unbounded), "unbounded_bg_s": round(sum(c.get("bg_dur") or 0 for c in unbounded)),
                      "fg_sleep_cmds": sum(1 for c in polls if not c.get("bg_dur") and (c.get("dur") or 0) > 30), "fg_sleep_s_over30": round(sum(c.get("dur") or 0 for c in polls if not c.get("bg_dur") and (c.get("dur") or 0) > 30))}
    res["auto_backgrounded"] = collections.Counter(str(c.get("auto_bg_timeout")) for c in bash if c.get("auto_bg_timeout"))

    # exploration
    ex = collections.Counter()
    for c in both:
        if c["name"] in ("Read", "Grep", "Glob"):
            ex[c["name"]] += 1
        elif c["name"] == "Bash" and c["cat"] == "shell explore (cat/grep/find/ls/sed)":
            ex["Bash cat/grep/find/ls/sed"] += 1
        elif c["name"] == "Bash" and c["cat"] == "codegraph (shell)":
            ex["codegraph (shell)"] += 1
        elif "codegraph" in c["name"]:
            ex["codegraph_explore (MCP)"] += 1
        elif c["name"] in ("Agent", "Task") and (c.get("subtype") in ("Explore",)):
            ex["Explore subagents"] += 1
    reread = {"files_read_multiple": 0, "extra_reads": 0, "extra_read_chars": 0, "full_rereads_unchanged": 0, "top": []}
    big = []
    topr = collections.Counter()
    sess_groups = collections.defaultdict(list)
    for c in both:
        sess_groups[(c["session"], c["kind"], c.get("kind") == "sub" and c["id"][:0])].append(c)
    for (sid, kind, _), cs in sess_groups.items():
        cnt = collections.Counter()
        edited_since = collections.defaultdict(bool)
        for c in cs:
            if c["name"] in ("Edit", "Write", "MultiEdit") and c.get("path"):
                edited_since[c["path"]] = True
            if c["name"] == "Read" and c.get("path"):
                p = c["path"]
                if cnt[p] > 0:
                    reread["extra_reads"] += 1
                    reread["extra_read_chars"] += c.get("len") or 0
                    if not edited_since[p] and not c.get("partial"):
                        reread["full_rereads_unchanged"] += 1
                cnt[p] += 1
                edited_since[p] = False
                if (c.get("len") or 0) > 25000:
                    big.append((c.get("len"), p))
        for p, n in cnt.items():
            if n > 1:
                reread["files_read_multiple"] += 1
                topr[(sid[:8], os.path.relpath(p, REPO))] = n
    reread["top"] = [{"session": k[0], "file": k[1], "reads": v} for k, v in topr.most_common(12)]
    readlens = [c.get("len") or 0 for c in both if c["name"] == "Read"]
    res["exploration"] = {"counts": dict(ex), "reread": reread,
                          "read_chars_total": sum(readlens), "reads_over_25k_chars": len(big),
                          "reads_over_25k_chars_total": sum(b[0] for b in big),
                          "big_read_files": collections.Counter(os.path.relpath(b[1], REPO) for b in big).most_common(10)}
    # result chars by tool (context pressure)
    rc = collections.Counter()
    for c in both:
        k = c["name"] if c["name"] != "Bash" else "Bash:" + c["cat"]
        rc[k] += c.get("len") or 0
    res["result_chars_by_tool"] = dict(rc.most_common(20))

    # subagents
    agents = [c for c in allcalls if c["name"] in ("Agent", "Task")]
    res["subagents"] = {
        "agent_calls_main": len(agents),
        "by_type": dict(collections.Counter(c.get("subtype") or "default" for c in agents)),
        "sum_fg_s": round(sum(c.get("dur") or 0 for c in agents)),
        "sum_bg_s": round(sum(c.get("bg_dur") or 0 for c in agents)),
        "n_background": sum(1 for c in agents if c.get("bg_dur") or (c.get("text_head") or "").find("async") >= 0),
        "transcripts": len(subs),
        "transcript_span_s": round(sum(s["span_s"] for s in subs.values())),
        "transcript_tool_s": round(sum(c.get("dur") or 0 for c in subcalls)),
        "transcript_tool_calls": len(subcalls),
        "transcript_tokens": dict(sum((collections.Counter(s["usage"]) for s in subs.values()), collections.Counter())),
        "longest": sorted([{"desc": c.get("desc"), "type": c.get("subtype"), "s": round(eff(c))} for c in agents], key=lambda x: -x["s"])[:8],
        "sub_types_meta": dict(collections.Counter((s.get("meta") or {}).get("agentType") for s in subs.values())),
    }

    # per session
    per = {}
    for sid, s in sessions.items():
        cs = s["calls"]
        per[sid[:8]] = {
            "span_h": round(s["span_s"] / 3600, 2),
            "active_turn_h": round(s["turn_ms"] / 3.6e6, 2),
            "human_msgs": len(s["human"]), "tool_calls": len(cs),
            "tool_fg_h": round(sum(c.get("dur") or 0 for c in cs) / 3600, 2),
            "bash_fg_h": round(sum(c.get("dur") or 0 for c in cs if c["name"] == "Bash") / 3600, 2),
            "test_h": round(sum(eff(c) for c in cs if c["name"] == "Bash" and c["cat"] in ("cargo test/nextest",)) / 3600, 2),
            "errors": sum(1 for c in cs if c.get("is_error")),
            "compactions": len(s["compacts"]),
            "assistant_msgs": s["n_assistant_msgs"],
            "tokens": s["usage"],
            "cost": s["cost"],
        }
    res["per_session"] = per
    tot_usage = sum((collections.Counter(s["usage"]) for s in sessions.values()), collections.Counter())
    res["totals"] = {
        "sessions": len(sessions),
        "span_h": round(sum(s["span_s"] for s in sessions.values()) / 3600, 1),
        "active_turn_h": round(sum(s["turn_ms"] for s in sessions.values()) / 3.6e6, 1),
        "tool_fg_h_main": round(sum(c.get("dur") or 0 for c in allcalls) / 3600, 2),
        "tool_calls_main": len(allcalls), "tool_calls_sub": len(subcalls),
        "human_msgs": sum(len(s["human"]) for s in sessions.values()),
        "assistant_msgs": sum(s["n_assistant_msgs"] for s in sessions.values()),
        "tokens_main": dict(tot_usage),
        "compactions": sum(len(s["compacts"]) for s in sessions.values()),
        "compactions_detail": collections.Counter(c["trigger"] for s in sessions.values() for c in s["compacts"]),
        "compact_pretokens_avg": round(sum((c["preTokens"] or 0) for s in sessions.values() for c in s["compacts"]) / max(1, sum(len(s["compacts"]) for s in sessions.values()))),
        "cost_usd": round(sum((s["cost"] or {}).get("totalCostUSD") or 0 for s in sessions.values()), 2),
        "cost_state_tool_h": round(sum((s["cost"] or {}).get("totalToolDuration") or 0 for s in sessions.values()) / 3.6e6, 1),
        "cost_state_api_h": round(sum((s["cost"] or {}).get("totalAPIDuration") or 0 for s in sessions.values()) / 3.6e6, 1),
    }

    # user corrections
    interrupts = 0
    corr = []
    for sid, s in sessions.items():
        for h in s["human"]:
            if h["text"].startswith("[Request interrupted"):
                interrupts += 1
            elif CORRECTION_RE.search(h["text"]):
                corr.append({"session": sid[:8], "text": h["text"][:400]})
    # interrupted tool calls
    res["user"] = {"interrupt_markers": interrupts,
                   "interrupted_tool_results": sum(1 for c in allcalls if c.get("interrupted") or c.get("err") == "user interrupted"),
                   "corrections_n": len(corr), "corrections": corr}
    with open(OUT, "w") as f:
        json.dump(res, f, indent=1, default=str)
    print("wrote", OUT)


main()
