#!/usr/bin/env bash
#
# Tests for guard-git-staging.sh. Run it directly:
#
#   bash .claude/hooks/guard-git-staging.test.sh
#
# Every case is a real command shape a session has produced or plausibly will.
# The false-positive cases matter as much as the denials: a guard that blocks
# writing about itself gets switched off, and then it protects nothing.

set -uo pipefail

hook="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/guard-git-staging.sh"
pass=0
fail=0

# check <expected: deny|allow> <description> <command>
check() {
  local expected=$1 desc=$2 command=$3 out verdict
  out=$(jq -n --arg c "$command" '{tool_input: {command: $c}}' | bash "$hook")
  if printf '%s' "$out" | grep -q '"deny"'; then verdict=deny; else verdict=allow; fi
  if [ "$verdict" = "$expected" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FAIL  expected %-5s got %-5s  %s\n      %s\n' "$expected" "$verdict" "$desc" "$command"
  fi
}

# --- Blanket staging -------------------------------------------------------
check deny "add -A" 'git add -A'
check deny "add --all" 'git add --all'
check deny "add ." 'git add .'
check deny "add -A with pathspec" 'git add -A -- vapor-app'
check deny "chained after another command" 'npm test && git add -A'
check deny "chained after a semicolon" 'cargo fmt; git add .'

# --- Committing everything -------------------------------------------------
check deny "commit -a" 'git commit -a -m "wip"'
check deny "commit -am" 'git commit -am "wip"'
check deny "commit -a after add" 'git add foo.rs && git commit -a -m "x"'

# --- Removing work from the tree -------------------------------------------
check deny "stash" 'git stash'
check deny "stash push with paths" 'git stash push vapor-app/src/App.tsx'
check deny "stash pop" 'git stash pop'

# --- Wholesale discards ----------------------------------------------------
check deny "reset --hard" 'git reset --hard'
check deny "reset --hard to a ref" 'git reset --hard origin/main'
check deny "checkout ." 'git checkout .'
check deny "restore ." 'git restore .'

# --- Must stay allowed -----------------------------------------------------
check allow "explicit paths" 'git add vapor-app/src/App.tsx docs/TESTING.md'
check allow "commit with a message" 'git commit -m "Fix the drag handler"'
check allow "amend, which is --a but not -a" 'git commit --amend --no-edit'
check allow "the word all inside a message" 'git commit -m "install all the deps"'
check allow "a path that starts with a dot" 'git add .github/workflows/ci.yml'
check allow "stash list is read-only" 'git stash list'
check allow "stash show is read-only" 'git stash show -p'
check allow "checkout of a branch" 'git checkout -b claude/split-lib'
check allow "checkout of one path" 'git checkout -- vapor-app/src/App.tsx'
check allow "reset without --hard" 'git reset HEAD~1'
check allow "status" 'git status --porcelain'
check allow "nothing to do with git" 'npm run test'

# --- Prose is not a command ------------------------------------------------
# The first commit describing this hook was denied by this hook. Its message
# quoted the commands it blocks; the shell never saw them as commands.
check allow "heredoc message quoting the blocked commands" \
  "$(printf 'git commit -F - <<%s\nBlocks git add -A and git commit -a and git stash.\nEOF' "'EOF'")"
check allow "unquoted heredoc delimiter" \
  "$(printf 'cat >> notes.md <<EOF\nNever run git add -A here.\nEOF')"
check allow "-m message quoting a blocked command" \
  'git commit -m "Explain why git add -A is blocked"'
check allow "single-quoted -m message" \
  "git commit -m 'document git stash and git reset --hard'"
# ...but a real command after a message must still be caught.
check deny "real command following a harmless message" \
  'git commit -m "explain the guard" && git add -A'
check deny "real command following a heredoc" \
  "$(printf 'git commit -F - <<%s\nA message about the guard.\nEOF\ngit add -A' "'EOF'")"

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
