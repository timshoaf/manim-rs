#!/usr/bin/env bash
# Verifies that every media file the book references actually exists.
#
# The deploy workflow runs this between the render harness and `mdbook build`,
# so a figure that silently failed to render fails the deploy instead of
# shipping a broken <video> tag. mdBook itself will happily build a page whose
# media is missing — that tolerance is deliberate (it keeps the CI smoke build
# asset-free) but it means the deploy path needs this explicit gate.
#
#   ./site/scripts/check-assets.sh
set -euo pipefail

cd "$(dirname "$0")/.."

missing=0
found=0

# Placeholder media satisfies "the file exists" while being a captioned colour
# card. If the harness partially fails, leftover placeholders would sail through
# every other check and ship to the site. Treat their marker as fatal in CI and
# as a loud warning locally.
if [ -f src/assets/.placeholders ]; then
    if [ -n "${CI:-}" ]; then
        echo "check-assets: FATAL — src/assets/.placeholders is present." >&2
        echo "  Placeholder media must never be deployed. The render harness" >&2
        echo "  either did not run or did not overwrite every figure." >&2
        exit 1
    fi
    echo "check-assets: WARNING — placeholder media present (src/assets/.placeholders)."
    echo "  These are colour cards, not renders. Run the harness before trusting the site."
fi

# Pull every src="assets/..." and poster="assets/..." out of the chapter
# sources, dedupe, and check each one resolves under src/.
refs=$(grep -rhoE '(src|poster)="assets/[^"]+"' src/*.md \
       | sed -E 's/^(src|poster)="//; s/"$//' \
       | sort -u)

if [ -z "$refs" ]; then
    echo "check-assets: no figure references found in src/*.md — is that right?" >&2
    exit 1
fi

while IFS= read -r ref; do
    if [ -f "src/$ref" ]; then
        found=$((found + 1))
    else
        echo "MISSING: site/src/$ref" >&2
        missing=$((missing + 1))
    fi
done <<< "$refs"

echo "check-assets: $found present, $missing missing"

if [ "$missing" -gt 0 ]; then
    cat >&2 <<'EOF'

Some figures referenced by the book were not produced by the render harness.

  - Locally, generate stand-ins:  ./site/scripts/placeholder-assets.sh
  - In CI, this means `cargo run -p render-examples --release` did not emit
    them. Check the harness manifest covers every example the book wires up.
EOF
    exit 1
fi
