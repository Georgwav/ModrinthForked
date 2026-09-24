#!/usr/bin/env bash
# Merges a Modrinth release into the current branch, keeping Threadrinth's
# changes. Paths listed in scripts/upstream-removed.txt are deleted again, so
# upstream changes to parts we don't ship never conflict.
#
# Usage: scripts/sync-upstream.sh [ref]   (default: upstream/main)
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
ref="${1:-upstream/main}"

if ! git diff --quiet || ! git diff --cached --quiet; then
	echo "Commit or stash your changes first." >&2
	exit 1
fi

git remote get-url upstream >/dev/null 2>&1 ||
	git remote add upstream https://github.com/modrinth/code.git
git fetch upstream --tags

git merge --no-ff --no-commit "$ref" || true

while IFS= read -r path || [ -n "$path" ]; do
	case "$path" in
	'' | '#'*) continue ;;
	/* | *..*) echo "Skipping unsafe path: $path" >&2 && continue ;;
	esac
	git rm -r -f -q --ignore-unmatch -- "$path" >/dev/null
	rm -rf -- "$path"
done <scripts/upstream-removed.txt

if ! git rev-parse -q --verify MERGE_HEAD >/dev/null; then
	echo "Already up to date with $ref."
	exit 0
fi

conflicts=$(git diff --name-only --diff-filter=U)
if [ -n "$conflicts" ]; then
	echo "Merged $ref, but these files need a manual fix:"
	echo "$conflicts"
	echo "Fix them, run 'git add' on them, then continue below."
fi

cat <<EOF

Next steps:
  1. pnpm install --lockfile-only   (drops removed packages from the lockfile)
  2. cargo check --workspace        (updates Cargo.lock)
  3. git add -A && git commit
EOF
