"""Slice the supplied RGBA logo sheet; preserve the original artwork and alpha.
Run with Python + Pillow, then `pnpm tauri icon design/app-icon-source.png`.
"""
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
source = Image.open(ROOT / 'design/reference/logo-sheet.png').convert('RGBA')
output = ROOT / 'public/brand'
output.mkdir(parents=True, exist_ok=True)
# Pixel rectangles refer to the original 1448 × 1086 transparent asset sheet.
slices = {
    'logo-mark.png': (58, 214, 388, 520),
    'wordmark.png': (408, 315, 1417, 466),
    'logo-lockup.png': (58, 214, 1417, 520),
    'app-icon.png': (59, 624, 419, 984),
}
for name, bounds in slices.items():
    source.crop(bounds).save(output / name, optimize=True)
# Keep the source's rounded tile, with optical padding for OS icon rendering.
tile = Image.open(output / 'app-icon.png').resize((896, 896), Image.Resampling.LANCZOS)
icon = Image.new('RGBA', (1024, 1024))
icon.alpha_composite(tile, (64, 64))
icon.save(ROOT / 'design/app-icon-source.png', optimize=True)
# Browser favicon uses the tile without extra OS padding at its small display size.
Image.open(output / 'app-icon.png').resize((64, 64), Image.Resampling.LANCZOS).save(ROOT / 'public/favicon.png')
