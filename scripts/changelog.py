#!/usr/bin/env python3
"""Builds docs/changelog.html from docs/changelog.toml and the git tags.

    python3 scripts/changelog.py [--check | --notes TAG]

Dates, times, lines and commits come from git; the words from the TOML. Every
tag needs an entry, or the build stops (--check only checks that; the entry for
the version in Cargo.toml may come before its tag). --notes prints a tag's entry
as Markdown for the GitHub release. The page is
self-contained: fonts (Instrument Serif/Sans, OFL, as in the README art) are
subset and inlined, so it opens offline and loads nothing from elsewhere.
Needs fontTools (python-fonttools).
"""

import base64
import datetime as dt
import html
import io
import math
import random
import re
import subprocess
import sys
import tomllib
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "docs" / "changelog.toml"
OUT = ROOT / "docs" / "changelog.html"
FONT_CACHE = Path.home() / ".cache" / "vela-readme-fonts"
FONTS = {
    "serif": "instrumentserif/InstrumentSerif-Italic.ttf",
    "sans": "instrumentsans/InstrumentSans%5Bwdth%2Cwght%5D.ttf",
}
UNICODES = "U+0020-007E,U+00A0-00FF,U+2013-2014,U+2018-201D,U+2022,U+2026,U+2190-2193,U+2212"
AREAS = {
    "launcher": ("Launcher", "#7aa2f7"),
    "panel": ("Control center", "#b4a0f5"),
    "hyprland": ("Hyprland", "#6fc3df"),
    "settings": ("Settings", "#c8d0ea"),
    "claude": ("Claude", "#d97757"),
    "pulse": ("Pulse", "#8fcf86"),
    "updates": ("Updates", "#e2b86b"),
    "nixos": ("NixOS", "#7ebae4"),
    "install": ("Install", "#a9b1d6"),
    "share": ("Share picker", "#ef8a9b"),
    "docs": ("Docs", "#8a93b8"),
}


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, check=True, capture_output=True, text=True).stdout.strip()


def tags():
    """Tags oldest first with their commit time and what changed since the previous one."""
    root = git("rev-list", "--max-parents=0", "HEAD").splitlines()[0]
    out, prev = [], root
    for name in git("tag", "--sort=creatordate").splitlines():
        stat = git("diff", "--shortstat", prev, name)
        num = lambda word: int(m.group(1)) if (m := re.search(rf"(\d+) {word}", stat)) else 0  # noqa: E731
        when = dt.datetime.fromisoformat(git("log", "-1", "--format=%cI", name))
        out.append(
            {
                "tag": name,
                "when": when,
                "lines": num("insertion"),
                "files": num("file"),
                "commits": int(git("rev-list", "--count", f"{prev}..{name}")),
            }
        )
        prev = name
    return out


def font_b64(key):
    from fontTools import subset

    FONT_CACHE.mkdir(parents=True, exist_ok=True)
    src = FONT_CACHE / FONTS[key].split("/")[-1]
    if not src.exists():
        urllib.request.urlretrieve("https://github.com/google/fonts/raw/main/ofl/" + FONTS[key], src)
    opts = subset.Options()
    opts.flavor = "woff"
    opts.layout_features = ["kern", "liga", "lnum", "tnum", "case"]
    font = subset.load_font(str(src), opts)
    sub = subset.Subsetter(opts)
    sub.populate(unicodes=subset.parse_unicodes(UNICODES))
    sub.subset(font)
    buf = io.BytesIO()
    subset.save_font(font, buf, opts)
    return base64.b64encode(buf.getvalue()).decode()


def inline(text):
    """Escapes, then turns `code` into <code>."""
    return re.sub(r"`([^`]+)`", r"<code>\1</code>", html.escape(text, quote=False))


def thousands(n):
    return f"{n:,}".replace(",", " ")


def starfield(count, seed, w=1600, h=1400):
    rnd = random.Random(seed)
    dots = []
    for _ in range(count):
        a = rnd.random() ** 3 * 0.75 + 0.12
        dots.append(f"{rnd.randrange(w)}px {rnd.randrange(h)}px 0 {rnd.choice(['0', '0', '0', '.5px'])} rgb(230 235 255 / {a:.2f})")
    return ",".join(dots)


def entry_html(t, e, max_lines):
    major, minor, patch = (int(x) for x in t["tag"].split("."))
    mag = math.log10(max(t["lines"], 2)) / math.log10(max_lines)
    big = patch == 0 and t["lines"] >= 1000
    classes = ["release"] + (["patch"] if patch else []) + (["big"] if big else [])
    areas = "".join(
        f'<li style="--c:{AREAS[a][1]}">{AREAS[a][0]}</li>' for a in e.get("area", [])
    )
    notes = "".join(f"<li>{inline(n)}</li>" for n in e.get("notes", []))
    lines = f"+{thousands(t['lines'])} line{'s' if t['lines'] != 1 else ''}"
    meta = f"{lines} · {t['files']} file{'s' if t['files'] != 1 else ''} · {t['commits']} commit{'s' if t['commits'] != 1 else ''}"
    when = t["when"]
    return f"""
      <li class="{' '.join(classes)}" id="v{t['tag']}" style="--m:{mag:.3f}">
        <span class="star" aria-hidden="true"></span>
        <article>
          <header>
            <h3><a href="#v{t['tag']}">{t['tag']}</a></h3>
            <time datetime="{when.isoformat()}">{when:%H:%M}</time>
            <ul class="areas" aria-label="Areas">{areas}</ul>
          </header>
          <p class="title">{inline(e['title'])}</p>
          {f'<ul class="notes">{notes}</ul>' if notes else ''}
          <p class="meta">{meta}</p>
        </article>
      </li>"""


def notes(tag):
    e = tomllib.loads(SRC.read_text()).get(tag)
    if e is None:
        sys.exit(f"docs/changelog.toml has no entry for {tag}")
    out = [f"**{e['title']}**", ""] + [f"- {n}" for n in e.get("notes", [])]
    areas = " · ".join(AREAS[a][0] for a in e.get("area", []))
    print("\n".join(out + ["", f"_{areas}_", "", "[Changelog](https://raindancer118.github.io/vela/)"]).replace("\n\n\n", "\n\n"))


def build():
    data = tomllib.loads(SRC.read_text())
    all_tags = tags()
    missing = [t["tag"] for t in all_tags if t["tag"] not in data]
    if missing:
        sys.exit(f"docs/changelog.toml has no entry for: {', '.join(missing)}")
    upcoming = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    unknown = [k for k in data if k not in ("intro", upcoming) and k not in {t["tag"] for t in all_tags}]
    if unknown:
        sys.exit(f"docs/changelog.toml has entries without a tag: {', '.join(unknown)}")
    if "--check" in sys.argv:
        print(f"docs/changelog.toml covers all {len(all_tags)} tags")
        return

    max_lines = max(t["lines"] for t in all_tags)
    days = {}
    for t in all_tags:
        days.setdefault(t["when"].date(), []).append(t)
    day_list = sorted(days, reverse=True)

    sections, nav = [], []
    for i, day in enumerate(day_list):
        rel = sorted(days[day], key=lambda t: t["when"], reverse=True)
        lines = sum(t["lines"] for t in rel)
        anchor = f"d{day:%Y-%m-%d}"
        nav.append(f'<li><a href="#{anchor}"><span>{day:%a}</span> {day.day} {day:%b}</a></li>')
        items = "".join(entry_html(t, data[t["tag"]], max_lines) for t in rel)
        sections.append(f"""
    <section class="day" id="{anchor}" aria-labelledby="{anchor}-h">
      <div class="day-head">
        <h2 id="{anchor}-h"><span class="weekday">{day:%A}</span><span class="date">{day.day} {day:%b}</span></h2>
        <p><span>{len(rel)} release{'s' if len(rel) != 1 else ''}</span> <span>+{thousands(lines)} lines</span></p>
      </div>
      <ol class="releases">{items}
      </ol>
    </section>""")
        # Days in between without a tag get a quiet mark on the rail.
        if i + 1 < len(day_list):
            gap = (day - day_list[i + 1]).days - 1
            for g in range(gap):
                quiet = day - dt.timedelta(days=g + 1)
                sections.append(f"""
    <div class="quiet"><span class="weekday">{quiet:%A}</span> <span class="date">{quiet.day} {quiet:%b}</span><p>Nothing tagged. A night off.</p></div>""")

    first, last = all_tags[0], all_tags[-1]
    span_days = (last["when"].date() - first["when"].date()).days + 1
    total_lines = int(git("diff", "--shortstat", git("rev-list", "--max-parents=0", "HEAD").splitlines()[0], last["tag"]).split(", ")[1].split()[0])
    total_commits = int(git("rev-list", "--count", last["tag"]))
    intro = data["intro"]
    page = TEMPLATE
    for key, value in {
        "SERIF": font_b64("serif"),
        "SANS": font_b64("sans"),
        "STARS_NEAR": starfield(90, 7),
        "STARS_FAR": starfield(160, 23),
        "TITLE": html.escape(intro["title"]),
        "LEDE": inline(intro["lede"]),
        "N_TAGS": str(len(all_tags)),
        "N_COMMITS": thousands(total_commits),
        "N_LINES": thousands(total_lines),
        "N_DAYS": str(span_days),
        "FIRST": first["tag"],
        "LAST": last["tag"],
        "NAV": "".join(nav),
        "SECTIONS": "".join(sections),
        "BUILT": f"{last['when']:%-d %B %Y}",
    }.items():
        page = page.replace("{{" + key + "}}", value)
    OUT.write_text(page)
    print(f"wrote {OUT.relative_to(ROOT)} ({len(all_tags)} tags, {OUT.stat().st_size // 1024} KiB)")


TEMPLATE = r"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="dark light">
<title>vela · changelog</title>
<link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 128 128'%3E%3Crect x='8' y='8' width='112' height='112' rx='28' fill='%2311141f'/%3E%3Cpath d='M60 28 L60 86 L30 86 Z' fill='%237aa2f7'/%3E%3Cpath d='M66 36 C82 50 90 68 90 86 L66 86 Z' fill='%23d97757'/%3E%3Cpath d='M26 92 H98 C94 100 86 104 76 104 H44 C36 104 30 100 26 92 Z' fill='%23e6ebff'/%3E%3C/svg%3E">
<style>
@font-face { font-family: "Instrument Serif"; font-style: italic; font-display: swap; src: url(data:font/woff;base64,{{SERIF}}) format("woff"); }
@font-face { font-family: "Instrument Sans"; font-weight: 400 700; font-stretch: 75% 100%; font-display: swap; src: url(data:font/woff;base64,{{SANS}}) format("woff"); }

:root {
  --ink: #e6ebff;
  --ink-2: #c8d0ea;
  --ink-3: #7d87a8;
  --sail: #a9c1ff;
  --sail-deep: #7aa2f7;
  --ember: #d97757;
  --night-0: #090c1c;
  --night-1: #0d1330;
  --night-2: #1a2257;
  --rail: rgb(169 193 255 / .28);
  --star: #f4f6ff;
  --glow: rgb(169 193 255 / .55);
  --serif: "Instrument Serif", "Iowan Old Style", "Palatino Linotype", Georgia, serif;
  --sans: "Instrument Sans", "Helvetica Neue", Arial, sans-serif;
  --col-day: 11.5rem;
  --col-rail: 3.5rem;
  color-scheme: dark;
}
@media (prefers-color-scheme: light) {
  :root {
    --ink: #151a3a;
    --ink-2: #39406a;
    --ink-3: #6c7396;
    --sail: #2b3a8f;
    --sail-deep: #3b55c9;
    --night-0: #f6f5fb;
    --night-1: #eef0fa;
    --night-2: #dfe4fb;
    --rail: rgb(43 58 143 / .22);
    --star: #2b3a8f;
    --glow: rgb(59 85 201 / .35);
    color-scheme: light;
  }
}

* { box-sizing: border-box; }
html { scroll-behavior: smooth; scroll-padding-top: 2rem; }
body {
  margin: 0;
  min-height: 100vh;
  background: var(--night-0);
  color: var(--ink);
  font: 400 1rem/1.6 var(--sans);
  font-feature-settings: "lnum";
  -webkit-font-smoothing: antialiased;
  overflow-x: hidden;
}

/* The sky: a deep gradient, two star layers that drift at different speeds, and
   the warm horizon from the README hero at the bottom of the window. */
.sky { position: fixed; inset: 0; z-index: -1; pointer-events: none; overflow: hidden;
  background:
    radial-gradient(120% 70% at 78% -10%, var(--night-2), transparent 60%),
    radial-gradient(90% 45% at 10% 115%, rgb(217 119 87 / .16), transparent 70%),
    linear-gradient(var(--night-1), var(--night-0)); }
.sky i { position: absolute; top: 0; left: 0; width: 1px; height: 1px; border-radius: 50%; }
.sky .far { box-shadow: {{STARS_FAR}}; opacity: .55; }
.sky .near { box-shadow: {{STARS_NEAR}}; }
@media (prefers-color-scheme: light) { .sky i { opacity: .12; filter: invert(1); } }

.page { width: min(70rem, 100% - 3rem); margin: 0 auto; padding: 6rem 0 4rem; }

/* ---------------------------------------------------------------- intro */
.intro { display: grid; grid-template-columns: 1fr minmax(0, 22rem); gap: 2rem 4rem; align-items: end; margin-bottom: 5.5rem; }
.kicker { display: flex; align-items: center; gap: .9rem; margin: 0 0 1.4rem; font: 600 .74rem/1 var(--sans); letter-spacing: .22em; text-transform: uppercase; color: var(--sail); }
.kicker svg { width: 2.6rem; height: 2.6rem; flex: none; }
.kicker::after { content: ""; width: 3rem; height: 1px; background: currentColor; opacity: .6; }
h1 { margin: 0; font: italic 400 clamp(4rem, 11vw, 8.5rem)/.86 var(--serif); letter-spacing: -.025em; color: var(--ink);
  background: linear-gradient(175deg, var(--ink) 30%, var(--sail) 120%); -webkit-background-clip: text; background-clip: text; -webkit-text-fill-color: transparent; padding-bottom: .08em; }
.lede { max-width: 34rem; margin: 1.6rem 0 0; font-size: 1.18rem; line-height: 1.55; color: var(--ink-2); text-wrap: pretty; }
.figures { display: grid; grid-template-columns: repeat(2, auto); gap: 1.4rem 2.6rem; margin: 0; justify-content: start; }
.figures div { display: flex; flex-direction: column-reverse; }
.figures dt { font: 600 .68rem/1.3 var(--sans); letter-spacing: .18em; text-transform: uppercase; color: var(--ink-3); }
.figures dd { margin: 0; font: italic 400 2.9rem/1 var(--serif); color: var(--ink); font-feature-settings: "lnum", "tnum"; }
.figures dd small { font-size: .45em; color: var(--ink-3); margin-left: .1em; }
.constellation { grid-column: 2; grid-row: 1; justify-self: end; width: 100%; max-width: 20rem; overflow: visible; }
.constellation path { fill: none; stroke: var(--rail); stroke-width: 1; }
.constellation circle { fill: var(--star); filter: drop-shadow(0 0 4px var(--glow)); }
.constellation text { font: italic 400 15px var(--serif); fill: var(--ink-3); }

nav.days { position: sticky; top: 0; z-index: 3; margin: 0 0 3.5rem; padding: .9rem 0; }
nav.days::before { content: ""; position: absolute; inset: 0 -100vw; z-index: -1; background: color-mix(in oklab, var(--night-0) 72%, transparent);
  backdrop-filter: blur(14px) saturate(140%); -webkit-backdrop-filter: blur(14px) saturate(140%); border-bottom: 1px solid var(--rail); }
nav.days ol { display: flex; flex-wrap: wrap; gap: .25rem 2rem; margin: 0; padding: 0; list-style: none; align-items: baseline; }
nav.days li:first-child::before { content: "Jump to"; margin-right: 1.4rem; font: 600 .68rem var(--sans); letter-spacing: .18em; text-transform: uppercase; color: var(--ink-3); }
nav.days a { display: inline-block; padding: .55rem 0; min-height: 48px; color: var(--ink-2); text-decoration: none; font: italic 400 1.35rem/1.3 var(--serif); transition: color .2s; }
nav.days a span { font: 600 .68rem var(--sans); letter-spacing: .16em; text-transform: uppercase; color: var(--ink-3); margin-right: .35rem; vertical-align: .18em; }
nav.days a:hover, nav.days a:focus-visible { color: var(--sail); }

/* ---------------------------------------------------------------- the rail */
.timeline { position: relative; }
.timeline::before, .timeline::after { content: ""; position: absolute; top: 0; bottom: 0; left: calc(var(--col-day) + var(--col-rail) / 2); width: 1px; translate: -.5px 0; }
.timeline::before { background: linear-gradient(transparent, var(--rail) 3%, var(--rail) 97%, transparent); }
/* The stretch of the course already sailed lights up as you scroll. */
.timeline::after { background: linear-gradient(var(--sail), var(--sail-deep)); box-shadow: 0 0 10px var(--glow); transform-origin: top; scale: 1 0; opacity: .8; }

.day { display: grid; grid-template-columns: var(--col-day) var(--col-rail) minmax(0, 1fr); }
.day-head { grid-column: 1; align-self: start; position: sticky; top: 6.5rem; padding: .3rem 1.5rem 3rem 0; text-align: right; }
.day-head h2 { margin: 0; display: flex; flex-direction: column; }
.weekday { font: 600 .68rem/1.4 var(--sans); letter-spacing: .2em; text-transform: uppercase; color: var(--sail); }
.date { font: italic 400 2.7rem/1 var(--serif); letter-spacing: -.01em; color: var(--ink); }
.day-head p span { display: block; }
.day-head p { margin: .7rem 0 0; font-size: .8rem; line-height: 1.5; color: var(--ink-3); font-feature-settings: "tnum"; }
.releases { grid-column: 2 / 4; margin: 0; padding: 0 0 4.5rem; list-style: none; }

.release { position: relative; display: grid; grid-template-columns: var(--col-rail) minmax(0, 1fr); padding-bottom: 2.6rem; }
.release article { grid-column: 2; max-width: 40rem; }
.star { grid-column: 1; justify-self: center; position: relative; margin-top: calc(1.15rem - var(--s) / 2);
  --s: calc(5px + var(--m) * 9px); width: var(--s); height: var(--s); border-radius: 50%;
  background: var(--star); box-shadow: 0 0 0 4px var(--night-0), 0 0 calc(4px + var(--m) * 22px) calc(var(--m) * 3px) var(--glow); }
.release.patch .star { background: var(--night-0); box-shadow: 0 0 0 3px var(--night-0), inset 0 0 0 1.5px var(--star), 0 0 8px var(--glow); }
/* The big releases get a four-point flare, like the brightest stars in the hero. */
.release.big .star::before, .release.big .star::after { content: ""; position: absolute; left: 50%; top: 50%; translate: -50% -50%;
  background: linear-gradient(90deg, transparent, var(--star), transparent); width: calc(var(--s) * 4.5); height: 1px; opacity: .7; }
.release.big .star::after { rotate: 90deg; }

.release header { display: flex; flex-wrap: wrap; align-items: baseline; gap: .2rem 1rem; }
.release h3 { margin: 0; font: italic 400 2.15rem/1.1 var(--serif); letter-spacing: -.01em; font-feature-settings: "lnum"; }
.release.big h3 { font-size: 3.1rem; }
.release.patch h3 { font-size: 1.7rem; color: var(--ink-2); }
.release h3 a { color: inherit; text-decoration: none; }
.release h3 a:hover { color: var(--sail); }
.release time { font-size: .8rem; color: var(--ink-3); font-feature-settings: "tnum"; }
.areas { display: flex; flex-wrap: wrap; gap: .1rem 1rem; margin: 0; padding: 0; list-style: none; }
.areas li { display: inline-flex; align-items: center; gap: .45rem; font: 600 .66rem/1.8 var(--sans); letter-spacing: .16em; text-transform: uppercase; color: var(--ink-3); }
.areas li::before { content: ""; width: .42rem; height: .42rem; border-radius: 50%; background: var(--c); box-shadow: 0 0 8px var(--c); }
.title { margin: .35rem 0 0; font: 560 1.2rem/1.4 var(--sans); letter-spacing: -.005em; color: var(--ink); text-wrap: balance; }
.release.big .title { font-size: 1.45rem; }
.notes { margin: .7rem 0 0; padding: 0; list-style: none; color: var(--ink-2); font-size: .98rem; }
.notes li { position: relative; padding-left: 1.3rem; margin-top: .35rem; text-wrap: pretty; }
.notes li::before { content: ""; position: absolute; left: 0; top: .78em; width: .65rem; height: 1px; background: var(--ink-3); }
.meta { margin: .8rem 0 0; font-size: .76rem; letter-spacing: .02em; color: var(--ink-3); font-feature-settings: "tnum"; }
code { font: .88em/1 ui-monospace, "JetBrains Mono", monospace; padding: .12em .38em; border-radius: .3em; background: color-mix(in oklab, var(--sail) 14%, transparent); color: var(--ink); }

.quiet { display: grid; grid-template-columns: var(--col-day) var(--col-rail) 1fr; align-items: baseline; padding: 0 0 4.5rem; color: var(--ink-3); }
.quiet .weekday { grid-column: 1; grid-row: 1; justify-self: end; padding-right: 1.5rem; color: var(--ink-3); }
.quiet .date { grid-column: 1; grid-row: 2; justify-self: end; padding-right: 1.5rem; font-size: 1.9rem; color: var(--ink-3); }
.quiet p { grid-column: 3; grid-row: 2; margin: 0; font: italic 400 1.35rem var(--serif); }

footer { margin-top: 2rem; padding-top: 2rem; border-top: 1px solid var(--rail); display: flex; flex-wrap: wrap; justify-content: space-between; gap: 1rem; font-size: .8rem; color: var(--ink-3); }
footer a { color: var(--ink-2); }

/* ---------------------------------------------------------------- motion */
@media (prefers-reduced-motion: no-preference) {
  .sky .near { animation: twinkle 7s ease-in-out infinite alternate; }
  .constellation circle { animation: twinkle 4s ease-in-out infinite alternate; }
  .constellation circle:nth-child(2n) { animation-delay: -2s; animation-duration: 5.5s; }
  @keyframes twinkle { from { opacity: .65; } to { opacity: 1; } }

  @supports (animation-timeline: view()) {
    .timeline::after { animation: sail linear both; animation-timeline: view(); animation-range: entry 0% exit 100%; }
    @keyframes sail { from { scale: 1 0; } to { scale: 1 1; } }
    .sky .far { animation: drift-far linear both; animation-timeline: scroll(root); }
    .sky .near { animation: drift-near linear both, twinkle 7s ease-in-out infinite alternate; animation-timeline: scroll(root), auto; }
    @keyframes drift-far { to { translate: 0 -120px; } }
    @keyframes drift-near { to { translate: 0 -320px; } }
    .release article { animation: rise linear both; animation-timeline: view(); animation-range: entry 5% entry 55%; }
    @keyframes rise { from { opacity: 0; translate: 0 1.5rem; } }
    .release .star { animation: ignite linear both; animation-timeline: view(); animation-range: entry 10% cover 45%; }
    @keyframes ignite { from { opacity: .15; scale: .4; } }
  }
}

/* ---------------------------------------------------------------- narrow */
@media (max-width: 52rem) {
  :root { --col-rail: 2rem; }
  .page { padding-top: 3.5rem; }
  .intro { grid-template-columns: 1fr; margin-bottom: 3.5rem; }
  .constellation { display: none; }
  .timeline::before, .timeline::after { left: calc(var(--col-rail) / 2); }
  .day { grid-template-columns: var(--col-rail) minmax(0, 1fr); }
  .day-head { grid-column: 2; position: static; text-align: left; padding: 0 0 1.6rem; }
  .day-head h2 { flex-direction: row; align-items: baseline; gap: .8rem; }
  .day-head p span { display: inline; }
  .day-head p span + span::before { content: "· "; }
  .releases { grid-column: 1 / 3; }
  .quiet { grid-template-columns: var(--col-rail) 1fr; }
  .quiet .weekday, .quiet .date { grid-column: 2; grid-row: auto; justify-self: start; }
  .quiet p { grid-column: 2; grid-row: auto; }
  .release.big h3 { font-size: 2.5rem; }
}
</style>
</head>
<body>
<div class="sky" aria-hidden="true"><i class="far"></i><i class="near"></i></div>
<div class="page">
  <header class="intro">
    <div>
      <p class="kicker"><svg viewBox="0 0 128 128" aria-hidden="true"><defs><linearGradient id="bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#2b3350"/><stop offset="1" stop-color="#11141f"/></linearGradient><linearGradient id="sl" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#a9c1ff"/><stop offset="1" stop-color="#7aa2f7"/></linearGradient></defs><rect x="8" y="8" width="112" height="112" rx="28" fill="url(#bg)"/><rect x="8.5" y="8.5" width="111" height="111" rx="27.5" fill="none" stroke="#fff" stroke-opacity=".12"/><path d="M60 28 L60 86 L30 86 Z" fill="url(#sl)"/><path d="M66 36 C82 50 90 68 90 86 L66 86 Z" fill="#d97757"/><path d="M26 92 H98 C94 100 86 104 76 104 H44 C36 104 30 100 26 92 Z" fill="#e6ebff" fill-opacity=".9"/></svg>vela · changelog</p>
      <h1>{{TITLE}}</h1>
      <p class="lede">{{LEDE}}</p>
    </div>
    <dl class="figures">
      <div><dt>Tags</dt><dd>{{N_TAGS}}</dd></div>
      <div><dt>Days</dt><dd>{{N_DAYS}}</dd></div>
      <div><dt>Commits</dt><dd>{{N_COMMITS}}</dd></div>
      <div><dt>Lines written</dt><dd>{{N_LINES}}</dd></div>
    </dl>
    <svg class="constellation" viewBox="0 0 340 200" aria-hidden="true">
      <path d="M12 113 L165 6 L212 41 L332 89 L262 180 L183 183 L111 179 Z"/>
      <circle cx="12" cy="113" r="2.4"/><circle cx="165" cy="6" r="2.8"/><circle cx="212" cy="41" r="2.2"/><circle cx="332" cy="89" r="3.4"/>
      <circle cx="262" cy="180" r="2.6"/><circle cx="183" cy="183" r="2.2"/><circle cx="111" cy="179" r="1.8"/>
      <text x="20" y="104">μ</text><text x="222" y="30">λ</text><text x="318" y="72">γ</text><text x="270" y="172">δ</text><text x="190" y="172">κ</text>
    </svg>
  </header>

  <nav class="days" aria-label="Days"><ol>{{NAV}}</ol></nav>

  <main class="timeline">{{SECTIONS}}
  </main>

  <footer>
    <span>{{FIRST}} → {{LAST}}, as of {{BUILT}}. Star size follows the lines a release added.</span>
    <span>Built from the git tags by <code>scripts/changelog.py</code> · <a href="https://github.com/Raindancer118/vela/releases">Releases on GitHub</a></span>
  </footer>
</div>
</body>
</html>
"""

if __name__ == "__main__":
    if "--notes" in sys.argv:
        notes(sys.argv[sys.argv.index("--notes") + 1])
    else:
        build()
