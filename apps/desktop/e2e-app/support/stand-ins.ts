// The programs the sandbox puts on PATH in place of real ones, as the
// daemon's own end-to-end tests do (crates/muster/tests/support/mod.rs).
// `@NAME@` is filled in with the sandbox's folders as it is made: the app's
// environment is fixed when it starts, and a stand-in must find its folder
// whatever it is given.

/**
 * Stands in for every coding agent, by the name it was started as. Asked
 * one question (naming a chat, with no terminal), it answers nothing.
 * Started in a session, it records how (its arguments, the hook socket and
 * token, its terminal, its folder, the id Claude Code is given), announces
 * its start through the `SessionStart` hook as Claude Code does, then
 * behaves as a shell. The tests make it ask, work and finish from outside
 * (support/agents.ts): hooks posted to the socket, text written to its
 * terminal, files written in its folder.
 */
export const STAND_IN_AGENT = `#!/bin/sh
[ -t 0 ] || exit 0
name=$(basename "$0")
dir="@AGENTS@/$MUSTER_SESSION"
mkdir -p "$dir"
n=$(( $(ls "$dir" | grep -c '\\.env$') + 1 ))
id=""; previous=""
for arg in "$@"; do
  case "$previous" in --session-id|--resume) id="$arg" ;; esac
  previous="$arg"
done
for arg in "$@"; do printf '%s\\0' "$arg"; done > "$dir/launch-$n.args"
printf '%s\\n' "$name" "$MUSTER_HOOK_SOCKET" "$MUSTER_HOOK_TOKEN" "$(tty)" "$(pwd)" "$id" "$$" \\
  > "$dir/launch-$n.part"
mv "$dir/launch-$n.part" "$dir/launch-$n.env"
if [ "$name" = claude ] && [ -n "$MUSTER_HOOK_TOKEN" ]; then
  printf '{"hook_event_name":"SessionStart","session_id":"%s","source":"startup"}' "$id" |
    curl -sS -m 3 --unix-socket "$MUSTER_HOOK_SOCKET" -H "Authorization: Bearer $MUSTER_HOOK_TOKEN" \\
      -H "Content-Type: application/json" --data-binary @- http://muster/hooks/claude >/dev/null 2>&1
fi
PS1='$ ' exec /bin/sh
`;

/**
 * Stands in for the GitHub CLI (crates/muster/tests/support/mod.rs FAKE_GH):
 * answers \`issue list\` (\`mine.json\` for the person's own, \`--assignee @me\`)
 * and \`issue view N\` from fixture files, records
 * \`issue close N\` for an issue it has, keeps one pull request (#17) once
 * one is created, and records every call. While \`gh-down\` exists it
 * answers as GitHub out of reach does.
 */
export const STAND_IN_GH = `#!/bin/sh
dir="@GH@"
printf '%s\\n' "$*" >> "$dir/calls"
if [ -f "$dir/gh-down" ]; then
  echo "error connecting to api.github.com" >&2; exit 1
fi
case "$1 $2" in
  "issue list")
    case "$*" in
      *"--assignee @me"*) cat "$dir/mine.json" ;;
      *) cat "$dir/issues.json" ;;
    esac ;;
  "issue view")
    if [ -f "$dir/issue-$3.json" ]; then cat "$dir/issue-$3.json"
    else echo "GraphQL: Could not resolve to an Issue with the number of $3." >&2; exit 1; fi ;;
  "issue close")
    if [ ! -f "$dir/issue-$3.json" ]; then
      echo "GraphQL: Could not resolve to an Issue with the number of $3." >&2; exit 1; fi
    echo "$3" >> "$dir/closed" ;;
  "pr list") if [ -f "$dir/pr.json" ]; then cat "$dir/pr.json"; else echo "[]"; fi ;;
  "pr create")
    cat > "$dir/pr-body"
    echo '[{"number":17,"url":"https://github.com/o/app/pull/17","state":"OPEN","isDraft":true}]' > "$dir/pr.json"
    echo "https://github.com/o/app/pull/17" ;;
  "pr ready") echo "$3 $4" >> "$dir/ready" ;;
  "pr view") echo '{"state":"OPEN"}' ;;
  "auth status") echo "Logged in to github.com" ;;
  *) echo "unexpected gh call: $*" >&2; exit 1 ;;
esac
`;

/** Keeps what was put on the clipboard, so nothing reaches the person's. */
export const STAND_IN_COPY = `#!/bin/sh
cat > "@CLIPBOARD@/copied"
`;

/**
 * \`muster\` as the app finds it (MUSTER_BIN): logs each start of the
 * daemon, refuses to start it while \`daemon-refused\` exists (so a test
 * can hold it out of reach, as a broken install would), and is the real
 * one otherwise.
 */
export const STAND_IN_MUSTER = `#!/bin/sh
if [ "$1" = daemon ]; then
  printf '%s\\n' "$*" >> "@RECORD@/daemon-starts"
  if [ -f "@RECORD@/daemon-refused" ]; then
    echo "muster: the test holds the daemon out of reach" >&2; exit 1
  fi
fi
exec "@MUSTER@" "$@"
`;
