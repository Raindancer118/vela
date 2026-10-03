#!/usr/bin/env python3
"""Draws the README artwork into docs/readme/.

    python3 -m venv /tmp/artvenv && /tmp/artvenv/bin/pip install uharfbuzz fonttools pillow
    /tmp/artvenv/bin/python scripts/readme-art.py

Text is set with HarfBuzz and stored as outlines, so the SVGs look the same
everywhere (GitHub shows SVGs as images: no web fonts). The fonts (OFL) are
downloaded to ~/.cache/vela-readme-fonts on first use.
"""

import math
import random
import urllib.request
from pathlib import Path

import uharfbuzz as hb
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "readme"
CACHE = Path.home() / ".cache" / "vela-readme-fonts"
FONTS = {
    "serif-italic": "instrumentserif/InstrumentSerif-Italic.ttf",
    "sans": "instrumentsans/InstrumentSans%5Bwdth%2Cwght%5D.ttf",
    # Greek star names (Instrument has no Greek).
    "greek": "notoserifdisplay/NotoSerifDisplay-Italic%5Bwdth%2Cwght%5D.ttf",
}


def font_file(key, wght=None):
    CACHE.mkdir(parents=True, exist_ok=True)
    src = CACHE / FONTS[key].split("/")[-1]
    if not src.exists():
        urllib.request.urlretrieve("https://github.com/google/fonts/raw/main/ofl/" + FONTS[key], src)
    if wght is None:
        return src
    out = CACHE / f"{key}-{wght}.ttf"
    if not out.exists():
        f = TTFont(src)
        instancer.instantiateVariableFont(f, {"wght": wght, "wdth": 100}, inplace=True)
        f.save(out)
    return out


class Face:
    def __init__(self, path):
        self.tt = TTFont(path)
        self.glyphs = self.tt.getGlyphSet()
        self.order = self.tt.getGlyphOrder()
        self.upem = self.tt["head"].unitsPerEm
        blob = hb.Blob.from_file_path(str(path))
        self.hb = hb.Font(hb.Face(blob))

    def shape(self, text, tracking=0):
        buf = hb.Buffer()
        buf.add_str(text)
        buf.guess_segment_properties()
        hb.shape(self.hb, buf, {"kern": True, "liga": True})
        out, x = [], 0
        for info, pos in zip(buf.glyph_infos, buf.glyph_positions):
            out.append((self.order[info.codepoint], x + pos.x_offset, pos.y_offset))
            x += pos.x_advance + tracking
        return out, x - tracking

    def path(self, text, size, x, y, tracking=0, anchor="start"):
        """SVG path data for `text` with its baseline at y."""
        shaped, width = self.shape(text, tracking * self.upem / size if size else 0)
        s = size / self.upem
        if anchor == "middle":
            x -= width * s / 2
        elif anchor == "end":
            x -= width * s
        pen = SVGPathPen(self.glyphs)
        for name, gx, gy in shaped:
            t = TransformPen(pen, (s, 0, 0, -s, x + gx * s, y - gy * s))
            self.glyphs[name].draw(t)
        return pen.getCommands(), width * s


def stars(seed, n, w, h, avoid=None):
    rnd = random.Random(seed)
    out = []
    while len(out) < n:
        x, y = rnd.uniform(0, w), rnd.uniform(0, h)
        if avoid and avoid[0] < x < avoid[2] and avoid[1] < y < avoid[3]:
            continue
        r = rnd.choice([0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.4])
        out.append((x, y, r, rnd.uniform(0.25, 0.9), rnd.uniform(0, 6), rnd.uniform(2.5, 6)))
    return out


# The Vela constellation (the sail of Argo): RA hours, Dec degrees.
VELA = {
    "γ": (8.158, -47.34),
    "δ": (8.745, -54.71),
    "κ": (9.368, -55.01),
    "φ": (9.948, -54.57),
    "μ": (10.779, -49.42),
    "p": (10.623, -48.23),
    "ψ": (9.511, -40.47),
    "λ": (9.133, -43.43),
}
VELA_LINES = ["γ", "δ", "κ", "φ", "μ", "p", "ψ", "λ", "γ"]
VELA_BRIGHT = {"γ": 3.2, "δ": 2.8, "λ": 2.7, "κ": 2.6, "μ": 2.2, "φ": 2.0, "ψ": 2.0, "p": 1.7}


def project(box):
    """Sky chart projection into box: RA hours → degrees, shrunk by cos(dec)
    so the figure keeps its shape; east (growing RA) is on the left."""
    x0, y0, x1, y1 = box
    mid = math.radians(sum(v[1] for v in VELA.values()) / len(VELA))
    raw = {k: (-ra * 15 * math.cos(mid), -dec) for k, (ra, dec) in VELA.items()}
    xs = [p[0] for p in raw.values()]
    ys = [p[1] for p in raw.values()]
    s = min((x1 - x0) / (max(xs) - min(xs)), (y1 - y0) / (max(ys) - min(ys)))
    ox = x0 + ((x1 - x0) - (max(xs) - min(xs)) * s) / 2
    oy = y0 + ((y1 - y0) - (max(ys) - min(ys)) * s) / 2
    return {k: (ox + (x - min(xs)) * s, oy + (y - min(ys)) * s) for k, (x, y) in raw.items()}


LOGO = """
  <g transform="translate({x} {y}) scale({s})">
    <rect x="8" y="8" width="112" height="112" rx="28" fill="url(#logo-bg)"/>
    <rect x="8.5" y="8.5" width="111" height="111" rx="27.5" fill="none" stroke="#fff" stroke-opacity=".14"/>
    <path d="M60 28 L60 86 L30 86 Z" fill="url(#logo-sail)"/>
    <path d="M66 36 C82 50 90 68 90 86 L66 86 Z" fill="#d97757"/>
    <path d="M26 92 H98 C94 100 86 104 76 104 H44 C36 104 30 100 26 92 Z" fill="#e6ebff" fill-opacity=".9"/>
  </g>"""


def hero(dark):
    W, H = 1280, 440
    serif = Face(font_file("serif-italic"))
    sans = Face(font_file("sans", 520))
    sans_b = Face(font_file("sans", 640))
    greek = Face(font_file("greek", 420))
    p = {
        "sky0": "#060913" if dark else "#fbf7f1",
        "sky1": "#0d1330" if dark else "#e9eefb",
        "glow": "#2b3a8f" if dark else "#c9d6ff",
        "warm": "#d97757",
        "star": "#ffffff" if dark else "#3a4a8a",
        "line": "#a9c1ff" if dark else "#4b63c8",
        "word0": "#ffffff" if dark else "#141a33",
        "word1": "#b9c9ff" if dark else "#3b54b8",
        "text": "#c8d0ea" if dark else "#38405e",
        "dim": "#7d87a8" if dark else "#7a809a",
        "sea": "#a9c1ff" if dark else "#4b63c8",
    }
    word_path, word_w = serif.path("vela", 168, 252, 228, tracking=-2)
    tag1, _ = sans.path("Launcher, control center and every Hyprland setting.", 25, 258, 288)
    tag2, _ = sans.path("One calm place. Live. Searchable. Ask Claude for the rest.", 25, 258, 322)
    chips_y = 372
    chips = []
    cx = 258
    for label in ["RUST", "GTK 4", "LIBADWAITA", "HYPRLAND 0.55+", "MCP"]:
        path, w = sans_b.path(label, 12.5, cx + 14, chips_y + 4.6, tracking=1.6)
        chips.append(
            f'<rect x="{cx:.1f}" y="{chips_y - 13}" width="{w + 28:.1f}" height="26" rx="13" class="chip"/>'
            f'<path d="{path}" class="chiptext"/>'
        )
        cx += w + 28 + 10

    pts = project((840, 64, 1190, 318))
    lines = " ".join(f"{'M' if i == 0 else 'L'}{pts[k][0]:.1f} {pts[k][1]:.1f}" for i, k in enumerate(VELA_LINES))
    seg_len = sum(
        math.dist(pts[a], pts[b]) for a, b in zip(VELA_LINES, VELA_LINES[1:])
    )
    vela_stars = "".join(
        f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{VELA_BRIGHT[k] * 4:.1f}" fill="url(#halo)" class="tw" style="animation-delay:{i * 0.7:.1f}s"/>'
        f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{VELA_BRIGHT[k] * 0.75:.2f}" fill="{p["star"]}"/>'
        for i, (k, (x, y)) in enumerate(pts.items())
    )
    labels = []
    for k in ["γ", "δ", "κ", "λ", "μ"]:
        x, y = pts[k]
        lp, _ = greek.path(k, 15, x + 10, y - 8)
        labels.append(f'<path d="{lp}" class="greek"/>')
    field = "".join(
        f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r * 0.8:.2f}" fill="{p["star"]}" opacity="{o:.2f}"'
        + (f' class="tw" style="animation-delay:{d:.1f}s;animation-duration:{dur:.1f}s"' if i % 3 == 0 else "")
        + "/>"
        for i, (x, y, r, o, d, dur) in enumerate(stars(7, 170, W, H - 70, avoid=(230, 120, 800, 400)))
    )
    caption, _ = sans.path("VELA · THE SAILS · 8h–11h  −40°…−56°", 10.5, 1185, 362, tracking=1.4, anchor="end")
    # The sea: three slow swells.
    def swell(y, a, phase):
        pts_ = []
        for i in range(0, 65):
            x = i * 20
            pts_.append(f"{x} {y + a * math.sin(i / 3.2 + phase):.1f}")
        return "M" + " L".join(pts_)

    sea = "".join(
        f'<path d="{swell(H - 34 + j * 9, 3 - j * 0.6, j)}" class="sea" style="opacity:{0.32 - j * 0.08:.2f};animation-delay:{-j * 2}s"/>'
        for j in range(3)
    )
    logo = LOGO.format(x=118, y=104, s=1.0)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" role="img" aria-label="vela — launcher, control center and every Hyprland setting">
  <title>vela</title>
  <defs>
    <linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{p['sky0']}"/>
      <stop offset="1" stop-color="{p['sky1']}"/>
    </linearGradient>
    <radialGradient id="glow" cx=".78" cy=".35" r=".55">
      <stop offset="0" stop-color="{p['glow']}" stop-opacity="{.55 if dark else .7}"/>
      <stop offset="1" stop-color="{p['glow']}" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="warm" cx=".18" cy="1.05" r=".55">
      <stop offset="0" stop-color="{p['warm']}" stop-opacity="{.38 if dark else .28}"/>
      <stop offset="1" stop-color="{p['warm']}" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="halo">
      <stop offset="0" stop-color="{p['star']}" stop-opacity=".55"/>
      <stop offset=".35" stop-color="{p['line']}" stop-opacity=".18"/>
      <stop offset="1" stop-color="{p['line']}" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="word" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="{p['word0']}"/>
      <stop offset="1" stop-color="{p['word1']}"/>
    </linearGradient>
    <linearGradient id="streak" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="{p['star']}" stop-opacity="0"/>
      <stop offset="1" stop-color="{p['star']}" stop-opacity=".95"/>
    </linearGradient>
    <linearGradient id="logo-bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#2b3350"/>
      <stop offset="1" stop-color="#11141f"/>
    </linearGradient>
    <linearGradient id="logo-sail" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#a9c1ff"/>
      <stop offset="1" stop-color="#7aa2f7"/>
    </linearGradient>
    <clipPath id="frame"><rect width="{W}" height="{H}" rx="22"/></clipPath>
  </defs>
  <style>
    .tw {{ animation: tw 4.5s ease-in-out infinite; }}
    @keyframes tw {{ 0%, 100% {{ opacity: 1; }} 50% {{ opacity: .25; }} }}
    /* Visible by default; animations only add the entrance (backwards fill),
       so viewers without CSS animation still show everything. */
    .cons {{ fill: none; stroke: {p['line']}; stroke-width: 1.1; stroke-opacity: .42; stroke-linejoin: round;
             stroke-dasharray: {seg_len:.0f}; animation: draw 3.2s .4s cubic-bezier(.6,0,.2,1) backwards; }}
    @keyframes draw {{ from {{ stroke-dashoffset: {seg_len:.0f}; }} to {{ stroke-dashoffset: 0; }} }}
    .greek {{ fill: {p['dim']}; animation: fade 1.2s 2.8s ease backwards; }}
    .rise {{ animation: rise 1.1s cubic-bezier(.2,.8,.2,1) backwards; }}
    @keyframes rise {{ from {{ opacity: 0; transform: translateY(14px); }} to {{ opacity: 1; transform: none; }} }}
    @keyframes fade {{ from {{ opacity: 0; }} to {{ opacity: 1; }} }}
    .tag {{ fill: {p['text']}; }}
    .chip {{ fill: {p['line']}; fill-opacity: {.10 if dark else .09}; stroke: {p['line']}; stroke-opacity: .32; }}
    .chiptext {{ fill: {p['line']}; }}
    .cap {{ fill: {p['dim']}; }}
    .sea {{ fill: none; stroke: {p['sea']}; stroke-width: 1.2; animation: sway 9s ease-in-out infinite alternate; }}
    @keyframes sway {{ from {{ transform: translateX(-20px); }} to {{ transform: translateX(0); }} }}
    .shoot {{ opacity: 0; animation: shoot 9s 3s linear infinite; }} /* the one thing that may stay hidden */
    @keyframes shoot {{ 0% {{ opacity: 0; transform: translate(0,0); }} 2% {{ opacity: 1; }} 9% {{ opacity: 0; transform: translate(-260px, 120px); }} 100% {{ opacity: 0; transform: translate(-260px, 120px); }} }}
  </style>
  <g clip-path="url(#frame)">
    <rect width="{W}" height="{H}" fill="url(#sky)"/>
    <rect width="{W}" height="{H}" fill="url(#glow)"/>
    <rect width="{W}" height="{H}" fill="url(#warm)"/>
    <g>{field}</g>
    <g class="shoot"><path d="M1080 40 L1180 -6" stroke="url(#streak)" stroke-width="1.6" stroke-linecap="round" transform="rotate(180 1130 17)"/></g>
    <path d="{lines}" class="cons"/>
    {vela_stars}
    {''.join(labels)}
    <path d="{caption}" class="cap"/>
    {sea}
    <g class="rise">{logo}</g>
    <path d="{word_path}" fill="url(#word)" class="rise" style="animation-delay:.15s"/>
    <g class="rise" style="animation-delay:.35s"><path d="{tag1}" class="tag"/><path d="{tag2}" class="tag"/></g>
    <g class="rise" style="animation-delay:.55s">{''.join(chips)}</g>
  </g>
  <rect x=".5" y=".5" width="{W - 1}" height="{H - 1}" rx="21.5" fill="none" stroke="{p['line']}" stroke-opacity=".18"/>
</svg>
"""


def heading(text, sub, dark, icon=None):
    """A section heading: big serif title, small caps kicker, hairline."""
    W, H = 1280, 120
    serif = Face(font_file("serif-italic"))
    sans = Face(font_file("sans", 600))
    fg = "#eef1fb" if dark else "#151a30"
    accent = "#a9c1ff" if dark else "#3b54b8"
    dim = "#7d87a8" if dark else "#7a809a"
    kicker, kw = sans.path(sub.upper(), 13, 0, 34, tracking=2.4)
    title, tw = serif.path(text, 58, 0, 96)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" role="img" aria-label="{text}">
  <title>{text}</title>
  <defs><linearGradient id="hair" x1="0" x2="1"><stop offset="0" stop-color="{accent}" stop-opacity=".7"/><stop offset="1" stop-color="{accent}" stop-opacity="0"/></linearGradient></defs>
  <path d="{kicker}" fill="{accent}"/>
  <path d="{title}" fill="{fg}"/>
  <rect x="{tw + 28:.0f}" y="78" width="{W - tw - 28:.0f}" height="1.2" fill="url(#hair)"/>
  <circle cx="{tw + 28:.0f}" cy="78.6" r="3" fill="{accent}"/>
  <path d="M{kw + 16:.0f} 29.5 h40" stroke="{dim}" stroke-opacity=".5"/>
</svg>
"""


SECTIONS = [
    ("launcher", "The launcher", "Tap Super"),
    ("center", "The control center", "Quick settings & notifications"),
    ("pulse", "Pulse, the task manager", "Apps · performance · diagnosis"),
    ("hyprland", "Hyprland, without the config file", "Every option · live"),
    ("claude", "Just say it", "Claude + MCP"),
    ("install", "Get it", "Two commands"),
    ("inside", "Under the hood", "How it works"),
]


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for dark in (True, False):
        mode = "dark" if dark else "light"
        (OUT / f"hero-{mode}.svg").write_text(hero(dark))
        for key, title, sub in SECTIONS:
            (OUT / f"h-{key}-{mode}.svg").write_text(heading(title, sub, dark))
    print("written to", OUT)


if __name__ == "__main__":
    main()
