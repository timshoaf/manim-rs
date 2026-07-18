#!/usr/bin/env bash
# Generates placeholder media for every figure the book references, so the site
# can be built and reviewed before the real render harness
# (tools/render-examples) is wired up.
#
# This is a DEVELOPMENT AID, never used by the deploy workflow — CI renders the
# real assets. Output goes to site/src/assets/, which is gitignored.
#
#   ./site/scripts/placeholder-assets.sh
#
# Needs ffmpeg. Each placeholder is a captioned 3-second colour card at 854x480,
# visibly marked so nobody mistakes one for a real render.
set -euo pipefail

cd "$(dirname "$0")/.."
ASSETS="src/assets"

if ! command -v ffmpeg >/dev/null; then
    echo "error: ffmpeg not found on PATH" >&2
    exit 1
fi

# domain:example:kind — mirrors the manifest in
# tools/render-examples/src/main.rs. `clip` emits only an .mp4 and `still` only
# a .png, exactly as the real harness does: getting this wrong here would hide
# a broken <video>/<img> reference behind a placeholder that the real render
# never produces.
FIGURES=(
    "fields:symplectic_vs_rk4:clip"
    "fields:kepler_orbits:clip"
    "materials:domain_coloring_gallery:still"
    "materials:heatmap_contours:still"
    "deformations:conformal_square:clip"
    "deformations:mobius_flow:clip"
    "surfaces:torus_curvature:still"
    "surfaces:geodesic_race:clip"
    "surfaces:trefoil_tube:clip"
    "quantum:wavepacket_barrier:clip"
    "quantum:hydrogen_orbitals:still"
    "quantum:bloch_gates:clip"
    "chem:caffeine:clip"
    "chem:nacl_lattice:still"
    "chem:orbital_isosurface:still"
    "nn:transformer_block:clip"
    "nn:loss_landscape_descent:clip"
    "volumetrics:dipole_field:clip"
    "volumetrics:stream_ribbons:clip"
    "volumetrics:tensor_glyph_field:still"
)

for entry in "${FIGURES[@]}"; do
    IFS=: read -r domain name kind <<< "$entry"
    mkdir -p "$ASSETS/$domain"

    label="PLACEHOLDER — $domain/$name"
    # drawtext needs colons and backslashes escaped in the text argument.
    text=$(printf '%s' "$label" | sed 's/[:\\]/\\&/g')
    draw="drawtext=text='${text}':fontcolor=white:fontsize=22:x=(w-text_w)/2:y=(h-text_h)/2"

    case "$kind" in
        still)
            ffmpeg -y -loglevel error \
                -f lavfi -i "color=c=0x1b1b2b:s=1280x720:d=1:r=30" \
                -vf "$draw" -frames:v 1 "$ASSETS/$domain/$name.png"
            echo "placeholder: $domain/$name.png"
            ;;
        clip)
            ffmpeg -y -loglevel error \
                -f lavfi -i "color=c=0x1b1b2b:s=1280x720:d=3:r=30" \
                -vf "$draw" -c:v libx264 -pix_fmt yuv420p -movflags +faststart \
                "$ASSETS/$domain/$name.mp4"
            echo "placeholder: $domain/$name.mp4"
            ;;
        *)
            echo "error: unknown kind '$kind' for $domain/$name" >&2
            exit 1
            ;;
    esac
done

# Leave a marker so check-assets.sh can tell placeholder media from real
# renders. Placeholders and real assets are both "files that exist", so a
# stale placeholder would otherwise satisfy every check while shipping a colour
# card to the site — which is exactly what happened once during development.
# The real harness writes only .png/.mp4, so this marker survives a partial
# harness run and correctly reports the result as tainted.
date -u +'%Y-%m-%dT%H:%M:%SZ' > "$ASSETS/.placeholders"
printf '%s\n' "${FIGURES[@]}" >> "$ASSETS/.placeholders"

echo
echo "Wrote ${#FIGURES[@]} placeholder figures to $ASSETS/ (gitignored)."
echo "Marked $ASSETS/.placeholders — check-assets.sh will refuse to pass in CI."
echo "Replace with real renders via: cargo run -p render-examples --release"
