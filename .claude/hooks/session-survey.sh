#!/usr/bin/env bash
#
# What every other session in this repository is doing, printed once at start.
#
# Worktrees isolate trees, not tasks. On 2026-08-21 two sessions independently
# diagnosed the same wry/WKWebView drag bug and wrote the same one-line fix to
# tauri.conf.json, hours apart, in separate worktrees — byte-identical, and
# neither knew. Isolation prevented the collision; only looking prevents the
# duplication, and nobody looks reliably. So this runs on its own.
#
# Emits a SessionStart payload: the text lands in the session's context.
# Prints nothing on stdout if it cannot work out where it is — a survey that
# errors into the context is worse than no survey.
#
# Run it by hand any time: bash .claude/hooks/session-survey.sh

set -uo pipefail

command -v git >/dev/null 2>&1 || exit 0
root=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
[ -n "$root" ] || exit 0
cd "$root" || exit 0

main=main
git show-ref --verify --quiet refs/heads/main || main=master

# Which lane a path belongs to. Order matters: android.rs and the two release
# documents sit inside broader globs and have to be claimed first.
lane_of() {
  case $1 in
  */src-tauri/src/android.rs | .github/workflows/* | vapor-app/scripts/* | tools/*) echo platform ;;
  docs/RELEASE.md | docs/ANDROID.md) echo platform ;;
  vapor-app/src/app.css | vapor-app/src/tokens.css | vapor-app/src/lib/core.ts | vapor-app/src/test/ipc.ts | vapor-app/src/lib/generated/*) echo seam ;;
  vapor-app/src-tauri/*) echo shell ;;
  vapor-app/src/screens/* | vapor-app/src/App.tsx) echo screens ;;
  vapor-app/src/components/*) echo components ;;
  vapor-core/*) echo core ;;
  docs/* | README.md | THIRD_PARTY_NOTICES.md | LICENSE | licenses/*) echo docs ;;
  *) echo other ;;
  esac
}

out=""
say() { out+="$1"$'\n'; }

lanes=""
seams=""

survey_tree() {
  local dir=$1 label=$2 dirty n
  dirty=$(git -C "$dir" status --porcelain 2>/dev/null | grep -v '^!!' | sed 's/^...//')
  n=$(printf '%s' "$dirty" | grep -c . )
  if [ "$n" -eq 0 ]; then
    say "  $label — clean"
  else
    say "  $label — $n file(s):"
    while read -r f; do
      [ -n "$f" ] || continue
      say "      $f  [$(lane_of "$f")]"
      lanes+=" $(lane_of "$f")"
      case $f in
      vapor-app/src-tauri/src/lib.rs) seams+=" lib.rs" ;;
      vapor-app/src/lib/core.ts) seams+=" core.ts" ;;
      vapor-app/src/app.css | vapor-app/src/tokens.css) seams+=" tokens" ;;
      vapor-app/src/test/ipc.ts) seams+=" ipc.ts" ;;
      vapor-app/src/lib/generated/*) seams+=" generated" ;;
      esac
    done < <(printf '%s\n' "$dirty" | grep . | head -12)
    [ "$n" -gt 12 ] && say "      … and $((n - 12)) more"
  fi
}

say "Other sessions in this repository, as of session start."
say ""
say "Working trees"
survey_tree "$root" "main checkout"

stale=""
unlanded=""
while read -r path _rest; do
  [ -n "$path" ] || continue
  [ "$path" = "$root" ] && continue
  name=${path##*/}
  survey_tree "$path" "$name"

  clean=$(git -C "$path" status --porcelain 2>/dev/null | grep -vc '^!!')
  head=$(git -C "$path" rev-parse HEAD 2>/dev/null)
  if [ "$clean" -eq 0 ] && [ -n "$head" ]; then
    if git merge-base --is-ancestor "$head" "$main" 2>/dev/null; then
      stale+=" $name"
    else
      unlanded+=" $name"
    fi
  fi
done < <(git worktree list --porcelain 2>/dev/null | awk '/^worktree /{print $2}')

ahead=$(git branch --no-merged "$main" --format='%(refname:short)' 2>/dev/null | tr '\n' ' ')
say ""
say "Branches not in $main: ${ahead:-none}"

if [ -n "$stale" ] || [ -n "$unlanded" ]; then
  say ""
  say "Worktrees to deal with — these sit outside every git status, so work"
  say "parked in one is invisible until somebody runs git worktree list."
  [ -n "$stale" ] && say "  merged and clean, safe to remove:$stale"
  [ -n "$unlanded" ] && say "  clean but holding commits not in $main:$unlanded"
fi

busy=$(printf '%s' "$lanes" | tr ' ' '\n' | grep . | sort -u | tr '\n' ' ')
free=""
for l in shell screens components core platform docs; do
  case " $busy " in *" $l "*) ;; *) free+=" $l" ;; esac
done
say ""
say "Lanes with uncommitted work: ${busy:-none}"
if [ -n "$free" ]; then
  say "Lanes nobody is in:$free — pick from these and you will not meet anyone."
else
  say "Every lane has work in it. Read the file lists above before choosing."
fi

if [ -n "$seams" ]; then
  s=$(printf '%s' "$seams" | tr ' ' '\n' | grep . | sort -u | tr '\n' ' ')
  say ""
  say "Seams currently touched: $s"
  case $s in
  *lib.rs*) say "  lib.rs is the one-session-at-a-time door to the backend. Someone is in it." ;;
  esac
fi

if command -v lsof >/dev/null 2>&1; then
  ports=$(lsof -nP -iTCP:1420 -iTCP:1421 -sTCP:LISTEN 2>/dev/null | awk 'NR>1 {print $1, $9}' | sort -u | tr '\n' '; ')
  say ""
  if [ -n "$ports" ]; then
    say "Ports in use: $ports"
    say "  Do not kill these. 1420 is pinned in tauri.conf.json — one tauri dev per"
    say "  machine. For e2e, take your own: VAPOR_E2E_PORT=1431 npm run e2e"
  else
    say "Ports 1420 and 1421 are free."
  fi
fi

if command -v jq >/dev/null 2>&1; then
  jq -n --arg c "$out" '{hookSpecificOutput: {hookEventName: "SessionStart", additionalContext: $c}}'
else
  printf '%s' "$out"
fi
