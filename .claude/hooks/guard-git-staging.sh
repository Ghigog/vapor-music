#!/usr/bin/env bash
#
# Blocks git commands that sweep up work this session did not do.
#
# The working tree is shared by two or three Claude sessions at a time. A
# blanket stage takes whatever another session had half-finished and commits it
# under this session's message; `stash` is worse, because it removes that work
# from the tree entirely. Three commits in this repository's history carry work
# their author did not do — 278cb5d, 050281f, eb5fa70 — and 278cb5d landed 63
# minutes after CLAUDE.md wrote the rule against it.
#
# The rule failed because it cannot be followed reliably. `git status` has no
# author column, so a session that has lost its early context genuinely cannot
# tell its own older edits from someone else's. This makes the rule mechanical
# instead of voluntary.
#
# Reads the PreToolUse payload on stdin. Exits 0 either way: silence allows the
# command, a JSON deny blocks it with a reason the model can act on.
#
# Tests: .claude/hooks/guard-git-staging.test.sh

set -uo pipefail

command -v jq >/dev/null 2>&1 || exit 0

cmd=$(jq -r '.tool_input.command // ""' 2>/dev/null) || exit 0
[ -n "$cmd" ] || exit 0

# Prose is not a command. Commit messages and documentation quote the very
# commands this hook blocks — the first commit describing the hook was denied
# by the hook, because its message contained the words `git commit -a`. Match
# against a copy with the message bodies removed: heredoc contents, and the
# quoted argument to -m. What is left is the part the shell actually runs.
#
# A heredoc body is data, not a command. The exception is `$(...)` inside an
# unquoted heredoc, which does execute; blanket staging written that way would
# get past this, and it is not a construction anything here produces.
probe=$(printf '%s' "$cmd" | awk '
  {
    if (in_hd) { line = $0; sub(/[[:space:]]+$/, "", line); if (line == delim) in_hd = 0; next }
    if (match($0, /<<-?[[:space:]]*("[^"]+"|'"'"'[^'"'"']+'"'"'|[A-Za-z_][A-Za-z0-9_]*)/)) {
      delim = substr($0, RSTART, RLENGTH)
      sub(/^<<-?[[:space:]]*/, "", delim)
      gsub(/["'"'"']/, "", delim)
      in_hd = 1
    }
    print
  }
')
probe=$(printf '%s' "$probe" | sed -E "s/-m[[:space:]]+'[^']*'/-m MSG/g; s/-m[[:space:]]+\"[^\"]*\"/-m MSG/g")

deny() {
  jq -n --arg reason "$1" '{
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "deny",
      permissionDecisionReason: $reason
    }
  }'
  exit 0
}

# `git add -A` / `git add --all` / `git add .`
if printf '%s' "$probe" | grep -Eq 'git[[:space:]]+add[[:space:]]+([^;&|]*[[:space:]])?(-A|--all|\.)([[:space:]]|$)'; then
  deny "Blocked: blanket staging in a repository shared by several Claude sessions.

Stage the paths you actually changed instead:
  git add path/to/file.rs path/to/other.tsx

If \`git status\` shows work you do not recognise, it is another session's — leave
it, and say so in your reply. You cannot tell your own older edits from someone
else's, so if you are unsure, it is not yours. \`?? test-results/\` is Playwright
output and belongs to nobody.

See CLAUDE.md and docs/DECISIONS.md §7."
fi

# `git commit -a`, `-am`, etc. The single-dash requirement keeps `--amend` out.
if printf '%s' "$probe" | grep -Eq 'git[[:space:]]+commit[[:space:]]+([^;&|]*[[:space:]])?-[a-zA-Z]*a'; then
  deny "Blocked: committing every modified file sweeps up another session's work
in flight along with yours.

Stage explicit paths first, then commit:
  git add path/to/file.rs
  git commit -m \"...\"

See CLAUDE.md and docs/DECISIONS.md §7."
fi

# `git stash` — the worst case, because it removes work from the tree entirely.
# `list` and `show` only read, so they stay allowed.
if printf '%s' "$probe" | grep -Eq 'git[[:space:]]+stash([[:space:]]|$)' &&
  ! printf '%s' "$probe" | grep -Eq 'git[[:space:]]+stash[[:space:]]+(list|show)([[:space:]]|$)'; then
  deny "Blocked: \`git stash\` takes every uncommitted change out of the tree,
including work belonging to other sessions running right now. They will watch
their edits disappear mid-task.

To get a clean tree for yourself, use a worktree instead:
  git worktree list        # what already exists
  # then start one for this task

See CLAUDE.md and docs/DECISIONS.md §7."
fi

# Wholesale discards. Same failure mode: destroys work you cannot attribute.
if printf '%s' "$probe" | grep -Eq 'git[[:space:]]+reset[[:space:]]+([^;&|]*[[:space:]])?--hard'; then
  deny "Blocked: \`git reset --hard\` discards every uncommitted change in the
shared tree, not only yours.

Undo your own work by path:
  git checkout -- path/to/file.rs

See CLAUDE.md and docs/DECISIONS.md §7."
fi

if printf '%s' "$probe" | grep -Eq 'git[[:space:]]+(checkout|restore)[[:space:]]+([^;&|]*[[:space:]])?\.([[:space:]]|$)'; then
  deny "Blocked: discarding the whole working tree throws away other sessions'
uncommitted work along with your own.

Name the paths you want to revert:
  git checkout -- path/to/file.rs

See CLAUDE.md and docs/DECISIONS.md §7."
fi

exit 0
