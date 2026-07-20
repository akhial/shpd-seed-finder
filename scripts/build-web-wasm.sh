#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

wasm_package="$repo_root/web/src/lib/wasm/pkg"
runtime_source="$repo_root/android/app/src/main/assets/third_party/shattered-pixel-dungeon"
runtime_destination="$repo_root/web/public/third_party/shattered-pixel-dungeon"

mkdir -p "$wasm_package"
wasm-pack build crates/seedfinder-wasm \
    --target web \
    --release \
    --out-dir "$wasm_package" \
    --out-name seedfinder

mkdir -p "$runtime_destination" "$repo_root/web/src/generated" "$repo_root/web/public/licenses"
for asset in \
    items.png \
    item_icons.png \
    LICENSE.txt \
    ATTRIBUTION.md \
    ASSET_MANIFEST.json \
    catalog-v3.3.8.json
do
    cp "$runtime_source/$asset" "$runtime_destination/$asset"
done
cp "$runtime_source/catalog-v3.3.8.json" "$repo_root/web/src/generated/catalog.json"
cp "$repo_root/COPYING" "$repo_root/web/public/licenses/COPYING.txt"
cp "$repo_root/NOTICE" "$repo_root/web/public/licenses/NOTICE.txt"

python3 "$repo_root/scripts/generate-sprite-bounds.py"

echo "Built browser WASM package in $wasm_package"
echo "Copied Shattered Pixel Dungeon runtime assets to $runtime_destination"
echo "Copied generated catalog and license texts into web/"
echo "Generated sprite bounds into web/src/generated/sprite-bounds.json"
