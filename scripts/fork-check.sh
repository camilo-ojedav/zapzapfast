#!/usr/bin/env bash
# Verifies the fork after a rebase onto upstream, before building:
#   1. every hook listed in scripts/fork-hooks.txt is still in its file;
#   2. no upstream identity came back in the places the fork overrides;
#   3. no new hard-coded identity appeared in an upstream file the fork
#      does not hook yet (reported, not failed: it may need a new hook).
# Run from the repository root. Exits non-zero on the first two.
set -u
cd "$(git rev-parse --show-toplevel)"
failed=0
cr=$(printf '\r')

while IFS='|' read -r file text; do
  # Tolerate a CRLF checkout of the hook list.
  text=${text%"$cr"}
  case "$file" in ''|'#'*) continue ;; esac
  if [ ! -f "$file" ]; then
    echo "MISSING FILE  $file"; failed=1
  elif ! grep -qF -- "$text" "$file"; then
    echo "LOST HOOK     $file: $text"; failed=1
  fi
done < scripts/fork-hooks.txt

# Upstream identities that must not reappear where the fork overrides them.
forbidden=(
  'src/archive/encryption.rs|"rocks.zapfast.ZapFast"'
  'src/updates.rs|crmne/zapfast'
  'src/updates/transfer.rs|crmne/zapfast/releases'
  'src/paths.rs|Self::of("zapfast")'
  'src/notify/windows.rs|"me.paolino.zapfast"'
  'src/autostart.rs|"me.paolino.zapfast"'
)
for entry in "${forbidden[@]}"; do
  file=${entry%%|*}; text=${entry#*|}
  if grep -qF -- "$text" "$file" 2>/dev/null; then
    echo "UPSTREAM ID   $file: $text"; failed=1
  fi
done

# New identity strings upstream may have added since the last sync.
echo "--- review: upstream identity literals outside the fork (may need a hook)"
grep -rnE '"(zapfast|ZapFast)"|me\.paolino\.zapfast"|rocks\.zapfast\.ZapFast"|crmne/zapfast' src build.rs \
  --include='*.rs' | grep -v '^src/fork' | grep -vE '#\[cfg\(test\)\]|assert|try_parse_from|fn .*test' || true

if [ "$failed" -ne 0 ]; then
  echo "fork-check: FAILED"; exit 1
fi
echo "fork-check: hooks intact"
