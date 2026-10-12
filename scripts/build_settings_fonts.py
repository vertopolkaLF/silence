"""Generate the static settings fonts embedded by src/settings_ui/fonts.rs.

GPUI's DirectWrite backend cannot set variable-font axes, so every weight the
settings UI uses becomes its own static face with an exact usWeightClass.
Bricolage Grotesque's optical-size axis is pinned per family: text sizes use
opsz 14 and the 24px page titles use the "Display" family at opsz 24, matching
the browser's automatic optical sizing.

Requires fontTools and brotli: python -m pip install fonttools brotli
Run from the repository root: python scripts/build_settings_fonts.py
"""

from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "assets" / "fonts"
OUT = SOURCE / "settings"

# (source woff2, family name, file stem, fixed axes, weights)
FACES = [
    ("PlusJakartaSans-latin.woff2", "Plus Jakarta Sans", "PlusJakartaSans", {}, [400, 500, 560, 600]),
    ("BricolageGrotesque-latin.woff2", "Bricolage Grotesque", "BricolageGrotesque", {"opsz": 14},
     [500, 600, 620, 650, 700, 760, 800]),
    ("BricolageGrotesque-latin.woff2", "Bricolage Grotesque Display", "BricolageGrotesqueDisplay",
     {"opsz": 24}, [650, 700, 800]),
]


def set_names(font: TTFont, family: str, weight: int) -> None:
    style = f"W{weight}"
    full = f"{family} {style}"
    postscript = f"{family.replace(' ', '')}-{style}"
    name = font["name"]
    for record in list(name.names):
        if record.nameID in (1, 2, 3, 4, 6, 16, 17, 21, 22, 25) or record.nameID >= 256:
            name.removeNames(nameID=record.nameID)
    # DirectWrite derives family names from the style words; keep every style
    # "Regular" and let usWeightClass carry the exact weight.
    for name_id, value in ((1, family), (2, "Regular"), (3, postscript), (4, full), (6, postscript),
                           (16, family), (17, "Regular"), (21, family), (22, "Regular")):
        name.setName(value, name_id, 3, 1, 0x409)
    font["OS/2"].usWeightClass = weight
    font["OS/2"].fsSelection = (font["OS/2"].fsSelection & ~0b1100001) | 0b1000000
    font["head"].macStyle = 0
    for table in ("STAT", "fvar", "avar", "gvar", "HVAR", "MVAR", "cvar"):
        if table in font:
            del font[table]


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for old in OUT.glob("*.ttf"):
        old.unlink()
    for source, family, stem, axes, weights in FACES:
        for weight in weights:
            font = TTFont(SOURCE / source)
            font.flavor = None
            static = instantiateVariableFont(font, {**axes, "wght": weight}, updateFontNames=False)
            set_names(static, family, weight)
            path = OUT / f"{stem}-{weight}.ttf"
            static.save(path)
            print(f"{path.relative_to(ROOT)} {path.stat().st_size // 1024} KiB")


if __name__ == "__main__":
    main()
