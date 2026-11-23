"""Builds every Alterix brand image from the master icon, assets/icon/alterix.svg:
the 1024 px PNG, the app icons (.ico, .icns and PNG sizes, via the Tauri CLI), the README
banner, the GitHub social card and the brand board (assets/make_brand_board.py).

Run from the project root (needs uv and Node; uv fetches Python and PySide6 into its own cache):
  uv run --python 3.12 --with PySide6 assets/make_assets.py
Add --guides <folder> to also write copies of the banner and card with their safe areas drawn on.
"""

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

from PySide6.QtCore import QByteArray, QPointF, QRectF, Qt
from PySide6.QtGui import QColor, QFont, QFontDatabase, QFontMetrics, QGuiApplication, QImage, QPainter, QPainterPath, QPen
from PySide6.QtSvg import QSvgRenderer

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"
MASTER = ASSETS / "icon" / "alterix.svg"
BRAND = json.loads((ASSETS / "brand.json").read_text(encoding="utf-8"))

NAME = BRAND["name"]
TAGLINE = BRAND["tagline"]
SUBLINE = BRAND["subline"]
DARK = BRAND["themes"]["dark"]
SANS = BRAND["fonts"]["ui"]
MONO = BRAND["fonts"]["mono"]


def font(families: list, pixel_size: float, weight: QFont.Weight) -> QFont:
    available = set(QFontDatabase.families())
    found = [f for f in families if f in available]
    result = QFont() if found else QFontDatabase.systemFont(QFontDatabase.SystemFont.GeneralFont)
    if found:
        result.setFamilies(found)
    result.setPixelSize(round(pixel_size))
    result.setWeight(weight)
    return result


def fit(base: QFont, text: str, max_width: float) -> QFont:
    """Shrinks the font 2 px at a time until the text fits max_width."""
    result = QFont(base)
    while QFontMetrics(result).horizontalAdvance(text) > max_width - 8 and result.pixelSize() > 12:
        result.setPixelSize(result.pixelSize() - 2)
    if result.pixelSize() < base.pixelSize():
        print(f'Warning: "{text}" shrank from {base.pixelSize()} to {result.pixelSize()} px to fit.')
    return result


def icon_renderer() -> QSvgRenderer:
    renderer = QSvgRenderer(QByteArray(MASTER.read_bytes()))
    if not renderer.isValid():
        sys.exit(f"Cannot read the master icon: {MASTER}")
    return renderer


def new_image(width: int, height: int, fill: QColor) -> tuple:
    image = QImage(width, height, QImage.Format.Format_ARGB32)
    image.fill(fill)
    painter = QPainter(image)
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    painter.setRenderHint(QPainter.RenderHint.TextAntialiasing)
    painter.setRenderHint(QPainter.RenderHint.SmoothPixmapTransform)
    return image, painter


def write_png(size: int, out: Path) -> None:
    image, painter = new_image(size, size, QColor(0, 0, 0, 0))
    icon_renderer().render(painter, QRectF(0, 0, size, size))
    painter.end()
    image.save(str(out))
    print(f"Wrote {out.relative_to(ROOT)} ({size} x {size} px)")


def app_icons() -> None:
    """Runs the Tauri CLI, keeps the desktop sizes and copies .ico / .icns beside the master."""
    icons = ROOT / "src-tauri" / "icons"
    npx = shutil.which("npx") or shutil.which("npx.cmd")
    if not npx:
        sys.exit("Node.js (npx) is needed to write the app icons.")
    subprocess.run([npx, "tauri", "icon", str(MASTER), "--output", str(icons)], cwd=ROOT, check=True, capture_output=True)
    for extra in ["android", "ios"]:
        shutil.rmtree(icons / extra, ignore_errors=True)
    for extra in icons.glob("Square*Logo.png"):
        extra.unlink()
    (icons / "StoreLogo.png").unlink(missing_ok=True)
    shutil.copy(icons / "icon.ico", ASSETS / "icon" / "alterix.ico")
    shutil.copy(icons / "icon.icns", ASSETS / "icon" / "alterix.icns")
    print("Wrote src-tauri/icons/ and assets/icon/alterix.ico, alterix.icns")


def banner(out: Path, guides: Path | None) -> None:
    """1280 x 320 px at 2x: rounded panel, icon left, name and tagline beside it, centred as a group."""
    s = 2
    width, height, radius = 1280 * s, 320 * s, 24 * s
    image, painter = new_image(width, height, QColor(0, 0, 0, 0))
    panel = QPainterPath()
    panel.addRoundedRect(QRectF(s / 2, s / 2, width - s, height - s), radius, radius)
    painter.fillPath(panel, QColor(DARK["bg"]))
    painter.setPen(QPen(QColor(DARK["border"]), s))
    painter.drawPath(panel)

    icon_size, gap = 176 * s, 40 * s
    name_font = font(SANS, 96 * s, QFont.Weight.DemiBold)
    tag_font = font(MONO, 24 * s, QFont.Weight.Normal)
    name_m, tag_m = QFontMetrics(name_font), QFontMetrics(tag_font)
    text_width = max(name_m.horizontalAdvance(NAME), tag_m.horizontalAdvance(TAGLINE))
    left = (width - (icon_size + gap + text_width)) / 2
    icon_renderer().render(painter, QRectF(left, (height - icon_size) / 2, icon_size, icon_size))

    line_gap = 18 * s
    block = name_m.capHeight() + line_gap + tag_m.ascent()
    top = (height - block) / 2
    text_left = left + icon_size + gap
    painter.setFont(name_font)
    painter.setPen(QColor(DARK["text"]))
    painter.drawText(QPointF(text_left, top + name_m.capHeight()), NAME)
    painter.setFont(tag_font)
    painter.setPen(QColor(DARK["text_secondary"]))
    painter.drawText(QPointF(text_left, top + name_m.capHeight() + line_gap + tag_m.ascent()), TAGLINE)
    painter.end()
    image.save(str(out))
    print(f"Wrote {out.relative_to(ROOT)} ({width} x {height} px)")
    if guides:
        g = image.copy()
        p = QPainter(g)
        p.setPen(QPen(QColor("#d1242f"), 2))
        p.drawLine(0, height // 2, width, height // 2)
        p.drawLine(width // 2, 0, width // 2, height)
        p.end()
        g.save(str(guides / "banner-guides.png"))


# Social card: fixed layout from GitHub's repo card template (same values as the standard script).
CARD_W, CARD_H, SAFE = 1280, 640, 80
ICON_SIZE, ICON_TOP, ICON_GAP = 168, 150, 40
NAME_PX, NAME_SPACING = 112, 98
TAGLINE_PX, TAGLINE_BASELINE = 34, 420
SUBLINE_PX, SUBLINE_BASELINE = 24, 476


def social_card(out: Path, guides: Path | None) -> bool:
    bg = QColor(DARK["bg"])
    image, painter = new_image(CARD_W, CARD_H, bg)
    safe_width = CARD_W - 2 * SAFE
    name_font = font(SANS, NAME_PX, QFont.Weight.DemiBold)
    name_font.setLetterSpacing(QFont.SpacingType.PercentageSpacing, NAME_SPACING)
    icon_part = ICON_SIZE + ICON_GAP
    name_font = fit(name_font, NAME, safe_width - icon_part)
    name_width = QFontMetrics(name_font).horizontalAdvance(NAME)
    left = (CARD_W - (icon_part + name_width)) / 2
    icon_renderer().render(painter, QRectF(left, ICON_TOP, ICON_SIZE, ICON_SIZE))
    cap = QFontMetrics(name_font).capHeight()
    painter.setFont(name_font)
    painter.setPen(QColor(DARK["text"]))
    painter.drawText(QPointF(left + icon_part - name_font.pixelSize() * 0.035, ICON_TOP + ICON_SIZE / 2 + cap / 2), NAME)

    def centred(text: str, families: list, px: int, color: str, y: float) -> None:
        f = fit(font(families, px, QFont.Weight.Normal), text, safe_width)
        painter.setFont(f)
        painter.setPen(QColor(color))
        painter.drawText(QPointF((CARD_W - QFontMetrics(f).horizontalAdvance(text)) / 2, y), text)

    centred(TAGLINE, MONO, TAGLINE_PX, DARK["text_secondary"], TAGLINE_BASELINE)
    centred(SUBLINE, MONO, SUBLINE_PX, DARK["text_muted"], SUBLINE_BASELINE)
    painter.end()
    image.save(str(out))
    print(f"Wrote {out.relative_to(ROOT)} ({CARD_W} x {CARD_H} px)")

    xs, ys = [], []
    for y in range(0, CARD_H):
        for x in range(0, CARD_W, 2):
            if image.pixelColor(x, y) != bg:
                xs.append(x)
                ys.append(y)
    inside = bool(xs) and min(xs) >= SAFE and min(ys) >= SAFE and max(xs) <= CARD_W - SAFE and max(ys) <= CARD_H - SAFE
    print("Social card safe area check " + ("passed" if inside else "FAILED"))
    if guides:
        g = image.copy()
        p = QPainter(g)
        p.setPen(QPen(QColor("#d1242f"), 2))
        p.drawRect(SAFE, SAFE, CARD_W - 2 * SAFE, CARD_H - 2 * SAFE)
        p.end()
        g.save(str(guides / "social-card-guides.png"))
    return inside


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--guides", type=Path, help="Folder for copies with guides drawn on.")
    parser.add_argument("--skip-app-icons", action="store_true", help="Do not run the Tauri CLI.")
    args = parser.parse_args()
    app = QGuiApplication(sys.argv[:1])  # noqa: F841 - Qt needs an application object for fonts.
    if args.guides:
        args.guides.mkdir(parents=True, exist_ok=True)
    write_png(1024, ASSETS / "icon" / "alterix.png")
    if not args.skip_app_icons:
        app_icons()
    banner(ASSETS / "banner.png", args.guides)
    ok = social_card(ASSETS / "social-card.png", args.guides)
    board = subprocess.run(
        [sys.executable, str(ASSETS / "make_brand_board.py"), str(ASSETS / "brand.json"), "--out", str(ASSETS / "brand-board.png")],
        cwd=ROOT,
    )
    return 0 if ok and board.returncode == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
