#!/usr/bin/env bash
# ast-grep skips empty files, so also enforce FE-35 by filename.
# Run from the repository root; include untracked files for local checks.
set -euo pipefail

failed=0
while IFS= read -r -d '' path; do
  [ -f "$path" ] || continue
  printf '%s:1 [tsx-no-test-files] Do not add .test.tsx files to this repository.\n' "$path"
  failed=1
done < <(git ls-files -z --cached --others --exclude-standard -- '*.test.tsx')

exit "$failed"
