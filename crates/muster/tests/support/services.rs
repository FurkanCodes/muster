//! Stand-ins for the git hosts' and trackers' command lines beside `gh`:
//! GitLab's `glab`.

use std::path::PathBuf;

use super::Sandbox;

/// Stands in for the GitLab CLI: answers `issue list`, `issue view N` and
/// an issue's notes (`api projects/:fullpath/issues/N/notes…`) from
/// fixture files, none where there is no `notes-N.json` (refused where
/// there is a `notes-N.fail`), records `issue close N`, keeps one merge request
/// (!17) once one is created, with how it was asked for, and records every
/// call. Like the real one, it pushes nothing.
pub(super) const FAKE_GLAB: &str = r#"#!/bin/sh
dir="$FAKE_GLAB_DIR"
printf '%s\n' "$*" >> "$dir/calls"
case "$1 $2" in
  "issue list") cat "$dir/issues.json" ;;
  "issue view")
    if [ -f "$dir/issue-$3.json" ]; then cat "$dir/issue-$3.json"
    else echo "GET https://gitlab.example.com/api/v4/projects/g%2Fapp/issues/$3: 404 Not Found" >&2; exit 1; fi ;;
  "issue close")
    if [ ! -f "$dir/issue-$3.json" ]; then echo "404 Not Found" >&2; exit 1; fi
    echo "$3" >> "$dir/closed"; echo "Closed issue #$3" ;;
  api\ projects/:fullpath/issues/*)
    iid=${2#projects/:fullpath/issues/}; iid=${iid%%/*}
    if [ -f "$dir/notes-$iid.fail" ]; then echo "GET …/notes: 403 Forbidden" >&2; exit 1
    elif [ -f "$dir/notes-$iid.json" ]; then cat "$dir/notes-$iid.json"; else echo "[]"; fi ;;
  "mr list") if [ -f "$dir/mr.json" ]; then cat "$dir/mr.json"; else echo "[]"; fi ;;
  "mr create")
    printf '%s\n' "$@" > "$dir/mr-create"
    echo '[{"iid":17,"web_url":"https://gitlab.example.com/g/app/-/merge_requests/17","state":"opened","draft":true}]' > "$dir/mr.json"
    echo "https://gitlab.example.com/g/app/-/merge_requests/17" ;;
  *) echo "unexpected glab call: $*" >&2; exit 1 ;;
esac
"#;

impl Sandbox {
    /// The stand-in `glab`'s directory: `issues.json`, `issue-<n>.json`,
    /// and what it recorded (`calls`, `closed`, `mr-create`).
    pub(crate) fn fake_glab(&self) -> PathBuf {
        self.root().join("fake-glab")
    }
}
