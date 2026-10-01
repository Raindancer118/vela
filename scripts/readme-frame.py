#!/usr/bin/env python3
"""Frames the screenshots from readme-shots.sh for the README (night-sky
backdrop like the hero, rounded window, soft shadow), builds the bento
gallery and the animated search demo.

    DEMO=1 scripts/readme-shots.sh && scripts/readme-shots.sh
    /tmp/artvenv/bin/python scripts/readme-frame.py
"""

import random
import subprocess
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont, ImageOps

SHOTS = Path("/tmp/vela-shots")
ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "readme"
FONT = Path.home() / ".cache" / "vela-readme-fonts" / "sans-560.ttf"


def backdrop(w, h, seed=3):
    """Deep navy with a blue glow top right, a warm one bottom left, stars."""
    img = ImageOps.colorize(Image.linear_gradient("L").resize((w, h)), "#070a15", "#111736")
    glow = Image.new("RGB", (w, h), (0, 0, 0))
    d = ImageDraw.Draw(glow)
    d.ellipse((w * 0.45, -h * 0.5, w * 1.35, h * 0.75), fill=(46, 64, 160))
    d.ellipse((-w * 0.35, h * 0.55, w * 0.45, h * 1.6), fill=(150, 72, 52))
    glow = glow.filter(ImageFilter.GaussianBlur(min(w, h) * 0.22))
    img = ImageChops.add(img, glow, scale=1.6)
    d = ImageDraw.Draw(img)
    rnd = random.Random(seed)
    for _ in range(int(w * h / 9000)):
        x, y = rnd.uniform(0, w), rnd.uniform(0, h)
        r = rnd.choice([0.6, 0.8, 1.0, 1.3])
        a = rnd.randint(70, 200)
        d.ellipse((x - r, y - r, x + r, y + r), fill=(a, a, min(255, a + 25)))
    return img


def rounded(img, r):
    mask = Image.new("L", img.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, img.width - 1, img.height - 1), r, fill=255)
    out = img.convert("RGBA")
    out.putalpha(mask)
    return out


def place(canvas, shot, x, y, r=22):
    """Window with shadow and hairline border onto canvas at x, y."""
    win = rounded(shot, r)
    shadow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    sm = Image.new("L", win.size, 0)
    ImageDraw.Draw(sm).rounded_rectangle((0, 0, win.width - 1, win.height - 1), r, fill=150)
    shadow.paste((0, 0, 0, 255), (x, y + 22), sm)
    shadow = shadow.filter(ImageFilter.GaussianBlur(34))
    canvas.alpha_composite(shadow)
    canvas.alpha_composite(win, (x, y))
    edge = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    ImageDraw.Draw(edge).rounded_rectangle((x, y, x + win.width - 1, y + win.height - 1), r, outline=(255, 255, 255, 34), width=2)
    canvas.alpha_composite(edge)


def framed(name, pad=72, crop=None):
    shot = Image.open(SHOTS / f"{name}.png").convert("RGB")
    if crop:
        shot = shot.crop(crop)
    w, h = shot.width + 2 * pad, shot.height + 2 * pad
    canvas = backdrop(w, h, seed=hash(name) % 100).convert("RGBA")
    place(canvas, shot, pad, pad - 10)
    return canvas


def save(img, name):
    path = OUT / name
    img.convert("RGB").save(path, optimize=True)
    subprocess.run(["magick", str(path), "-strip", "-define", "png:compression-level=9", str(path)], check=False)
    print(f"  {name}  {path.stat().st_size // 1024} KiB")


def spaced(text):
    # Letter spacing for small caps (PIL has none): hair spaces.
    return "\u200a".join(text)


def label(draw, xy, text, size, fill, anchor="la"):
    draw.text(xy, text, font=ImageFont.truetype(str(FONT), size), fill=fill, anchor=anchor)


def bento():
    """Four pages side by side, each with a caption, on one night sky."""
    # Content side of the window (no sidebar, header kept); the dialog sits
    # in the middle of the whole window, so its cut is shifted left.
    content = (306, 0, 1272, 740)
    tiles = [
        ("monitors", "Monitors", "drag to arrange · keep or revert", content),
        ("effects", "Blur & effects", "every slider applies live", content),
        ("shortcuts", "Shortcuts", "record keys · take over hyprland.lua", content),
        ("rule-dialog", "Window rules", "pick an open window · 24 effects", (262, 0, 1228, 740)),
    ]
    crop = content
    cw, ch = crop[2] - crop[0], crop[3] - crop[1]
    gap, pad, cap = 56, 72, 118
    W = pad * 2 + cw * 2 + gap
    H = pad * 2 + (ch + cap) * 2 + gap
    canvas = backdrop(W, H, seed=11).convert("RGBA")
    d = ImageDraw.Draw(canvas)
    for i, (name, title, sub, cut) in enumerate(tiles):
        col, row = i % 2, i // 2
        x = pad + col * (cw + gap)
        y = pad + row * (ch + cap + gap)
        shot = Image.open(SHOTS / f"{name}.png").convert("RGB").crop(cut)
        place(canvas, shot, x, y, r=20)
        label(d, (x + 4, y + ch + 52), title, 36, (238, 241, 251))
        label(d, (x + 6, y + ch + 100), "  ".join(sub.upper()).replace("     ", "   ·   ") if False else spaced(sub.upper()), 17, (169, 193, 255))
    return canvas


def launcher_panel(name):
    """A launcher shot without the transparent margin around the panel
    (18 logical px, where the desktop shows through)."""
    img = Image.open(SHOTS / f"{name}.png").convert("RGB")
    scale = float((SHOTS / "scale").read_text()) if (SHOTS / "scale").exists() else 1.0
    inset = round(18 * scale) + 1
    return img.crop((inset, inset, img.width - inset, img.height - inset))


def launcher_strip():
    """Grid, app results and Ask Claude as a fanned deck, each a little
    lower and further right than the one before."""
    crops = [launcher_panel(n) for n in ("launcher-grid", "launcher-apps", "launcher-claude")]
    pad, dx, dy = 70, 300, 150
    W = pad * 2 + max(c.width + i * dx for i, c in enumerate(crops))
    H = pad * 2 + max(c.height + i * dy for i, c in enumerate(crops))
    canvas = backdrop(W, H, seed=5).convert("RGBA")
    for i, c in enumerate(crops):
        place(canvas, c, pad + i * dx, pad + i * dy, r=25)
    return canvas


def demo_webp():
    frames = sorted(SHOTS.glob("demo-*.png"))
    if not frames:
        print("  (no demo frames; run DEMO=1 scripts/readme-shots.sh)")
        return
    tmp = Path("/tmp/vela-demo")
    tmp.mkdir(exist_ok=True)
    for f in tmp.glob("*.png"):
        f.unlink()
    for i, f in enumerate(frames):
        framed_img = framed(f.stem, pad=48)
        framed_img.convert("RGB").resize((framed_img.width * 900 // framed_img.width, framed_img.height * 900 // framed_img.width), Image.LANCZOS).save(tmp / f"{i:02d}.png")
    # Holds (seconds): the empty start, the finished German sentence, the end.
    names = [f.stem for f in frames]
    holds = {0: 1.6, 7: 3.4, 8: 1.4, len(names) - 1: 3.2}
    concat = tmp / "list.txt"
    lines = []
    for i, _ in enumerate(names):
        lines += [f"file '{tmp / f'{i:02d}.png'}'", f"duration {holds.get(i, 0.32)}"]
    lines.append(f"file '{tmp / f'{len(names) - 1:02d}.png'}'")  # concat needs the last twice
    concat.write_text("\n".join(lines) + "\n")
    out = OUT / "demo-search.webp"
    subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i", str(concat),
         "-vf", "fps=25", "-c:v", "libwebp_anim", "-quality", "82", "-compression_level", "6", "-loop", "0", str(out)],
        check=True,
    )
    print(f"  demo-search.webp  {out.stat().st_size // 1024} KiB")


def main():
    if not FONT.exists():
        import importlib.util

        spec = importlib.util.spec_from_file_location("art", Path(__file__).with_name("readme-art.py"))
        art = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(art)
        art.font_file("sans", 560)
    OUT.mkdir(parents=True, exist_ok=True)
    save(framed("home"), "shot-home.png")
    save(framed("search-de"), "shot-search.png")
    save(framed("all"), "shot-all-options.png")
    save(bento(), "bento-hyprland.png")
    save(launcher_strip(), "launcher.png")
    demo_webp()


if __name__ == "__main__":
    main()
