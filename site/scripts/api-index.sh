#!/usr/bin/env bash
# Writes a landing page at <api-dir>/index.html.
#
# `cargo doc --no-deps` over a workspace emits one directory per crate but no
# top-level index, so /api/ would 404. Rather than blind-redirecting to one
# crate, this lists the workspace crates that actually got documented and links
# back to the book prominently.
#
#   ./site/scripts/api-index.sh _site/api
set -euo pipefail

API_DIR="${1:?usage: api-index.sh <api-dir>}"
[ -d "$API_DIR" ] || { echo "error: $API_DIR is not a directory" >&2; exit 1; }

# Crates we expect, in reading order. Only those present are listed.
CRATES=(
    "manim:The facade — prelude, render(), preview()"
    "manim_core:Mobject model, animations, scene runtime, Material"
    "manim_render:wgpu pipelines, offscreen renderer, exporters"
    "manim_math:Paths, Béziers, geometry"
    "manim_color:Colors and palettes"
    "manim_text:Text, TeX (typst), Code"
    "manim_fields:AD, fields, SpaceMap, integrators, PDE"
    "manim_sci:Fields → mobjects: deformation, materials, surfaces, volumetrics"
    "manim_quantum:Wavefunctions, eigenstates, Bloch sphere"
    "manim_chem:Molecules, lattices, orbitals"
    "manim_nn:Compute graphs, heatmaps, loss landscapes"
    "manim_dioxus:ManimPlayer, Figure, interaction widgets"
)

rows=""
for entry in "${CRATES[@]}"; do
    name="${entry%%:*}"
    desc="${entry#*:}"
    if [ -d "$API_DIR/$name" ]; then
        rows+="<tr><td><a href=\"./${name}/index.html\"><code>${name}</code></a></td><td>${desc}</td></tr>"$'\n'
    fi
done

if [ -z "$rows" ]; then
    echo "error: no documented crates found under $API_DIR" >&2
    exit 1
fi

cat > "$API_DIR/index.html" <<EOF
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>manim-rs — API reference</title>
<style>
  :root { color-scheme: light dark; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    line-height: 1.6; max-width: 48rem; margin: 0 auto; padding: 2rem 1.25rem;
  }
  h1 { margin-bottom: 0.25rem; }
  .sub { opacity: 0.7; margin-top: 0; }
  .book-link {
    display: block; margin: 1.5rem 0; padding: 1rem 1.25rem;
    border: 1px solid currentColor; border-radius: 6px;
    text-decoration: none; font-weight: 600; font-size: 1.05rem;
  }
  table { border-collapse: collapse; width: 100%; margin-top: 1.5rem; }
  th, td { text-align: left; padding: 0.5rem 0.6rem; border-bottom: 1px solid rgba(128,128,128,0.3); }
  td:first-child { white-space: nowrap; }
  code { font-size: 0.95em; }
</style>
</head>
<body>
  <h1>manim-rs — API reference</h1>
  <p class="sub">rustdoc for every crate in the workspace.</p>

  <a class="book-link" href="../">📖 &nbsp;Back to the manim-rs book — guides, worked examples, and rendered figures</a>

  <table>
    <thead><tr><th>Crate</th><th>What it is</th></tr></thead>
    <tbody>
${rows}    </tbody>
  </table>

  <p style="margin-top:2rem"><a href="../demos/">▶ Interactive demos</a> &middot;
     <a href="https://github.com/cryptex-ai/manim_rust">Source on GitHub</a></p>
</body>
</html>
EOF

echo "api-index: wrote $API_DIR/index.html"
