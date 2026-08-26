"""Render the Markdown and offline HTML galleries from the sample catalog."""

from __future__ import annotations

import argparse
import csv
from collections import Counter
from dataclasses import dataclass
from html import escape
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SAMPLES = ROOT / "samples"
CATALOG = SAMPLES / "catalog.tsv"
MARKDOWN_OUTPUT = SAMPLES / "README.md"
HTML_OUTPUT = SAMPLES / "index.html"
EXPECTED_SAMPLES = 115
EXPECTED_FEATURED = 20


@dataclass(frozen=True)
class Sample:
    filename: str
    category: str
    featured_order: int | None
    title: str
    description: str
    techniques: str

    @property
    def stem(self) -> str:
        return Path(self.filename).stem

    @property
    def image(self) -> str:
        return f"showcase/{self.stem}.png"

    @property
    def anchor(self) -> str:
        """Stable full-catalog anchor, independent of title and order."""
        return f"sample-{self.stem}"

    @property
    def highlight_anchor(self) -> str:
        """Stable showcase-card anchor, independent of title and order."""
        return f"highlight-{self.stem}"

    @property
    def technique_list(self) -> list[str]:
        return [technique.strip() for technique in self.techniques.split(";")]


def read_catalog() -> list[Sample]:
    with CATALOG.open(encoding="utf-8-sig", newline="") as handle:
        reader = csv.DictReader(handle, delimiter="\t")
        expected = [
            "filename",
            "category",
            "featured_order",
            "title",
            "description",
            "techniques",
        ]
        if reader.fieldnames != expected:
            raise ValueError(f"Unexpected catalog header: {reader.fieldnames}")
        samples = [
            Sample(
                filename=row["filename"].strip(),
                category=row["category"].strip(),
                featured_order=(
                    int(row["featured_order"])
                    if row["featured_order"].strip()
                    else None
                ),
                title=row["title"].strip(),
                description=row["description"].strip(),
                techniques=row["techniques"].strip(),
            )
            for row in reader
        ]
    return samples


def featured_samples(samples: list[Sample]) -> list[Sample]:
    return sorted(
        (sample for sample in samples if sample.featured_order is not None),
        key=lambda sample: sample.featured_order or 0,
    )


def category_names(samples: list[Sample]) -> list[str]:
    categories: list[str] = []
    for sample in samples:
        if sample.category not in categories:
            categories.append(sample.category)
    return categories


def validate(samples: list[Sample]) -> None:
    actual_files = {path.name for path in SAMPLES.glob("*.bas")}
    catalog_files = [sample.filename for sample in samples]
    if len(samples) != EXPECTED_SAMPLES:
        raise ValueError(f"Expected {EXPECTED_SAMPLES} samples, found {len(samples)}")
    if len(set(catalog_files)) != len(catalog_files):
        duplicates = [name for name, count in Counter(catalog_files).items() if count > 1]
        raise ValueError(f"Duplicate catalog entries: {duplicates}")
    if set(catalog_files) != actual_files:
        raise ValueError(
            "Catalog/files mismatch: "
            f"missing={sorted(actual_files - set(catalog_files))}, "
            f"extra={sorted(set(catalog_files) - actual_files)}"
        )
    featured = sorted(
        sample.featured_order
        for sample in samples
        if sample.featured_order is not None
    )
    if featured != list(range(1, EXPECTED_FEATURED + 1)):
        raise ValueError(f"Featured order must be 1..{EXPECTED_FEATURED}: {featured}")
    missing_images = [
        sample.image
        for sample in samples
        if sample.featured_order is not None and not (SAMPLES / sample.image).is_file()
    ]
    if missing_images:
        raise ValueError(f"Missing showcase images: {missing_images}")


def markdown_card(sample: Sample) -> list[str]:
    techniques = sample.techniques.replace(";", " · ")
    return [
        '<td width="50%" valign="top">',
        f'  <a id="{sample.highlight_anchor}"></a>',
        f'  <a href="{sample.filename}"><img src="{sample.image}" '
        f'alt="{sample.title} running in AVL BASIC" width="100%"></a><br>',
        f"  <strong>{sample.featured_order}. {sample.title}</strong><br>",
        f"  {sample.description}<br>",
        f"  <sub><strong>Shows:</strong> {techniques}</sub><br>",
        f'  <code>RUN "/samples/{sample.filename}"</code>',
        "</td>",
    ]


def markdown_escape(text: str) -> str:
    return text.replace("|", "\\|")


def render_markdown(samples: list[Sample]) -> str:
    featured = featured_samples(samples)
    categories = category_names(samples)
    lead = featured[0]
    lines = [
        "# AVL BASIC sample gallery",
        "",
        "AVL BASIC ships with **115 runnable programs**. They are not filler or API",
        "snippets: the collection includes complete visual pieces, playable programs,",
        "numerical algorithms, interactive explorers, and focused teaching examples.",
        "",
        "For the richest offline view, open [`samples/index.html`](index.html) in any",
        "modern browser. It needs no web server and keeps all 20 highlights and the",
        "complete catalog on one responsive page.",
        "",
        "From the interpreter, type:",
        "",
        "```basic",
        "TOUR",
        "SAMPLES",
        'RUN "/samples/g-old-school.bas"',
        "```",
        "",
        f"`TOUR` presents the {EXPECTED_FEATURED} highlights below. `SAMPLES` lists",
        "the entire catalog. Neither command changes the current program or runs",
        "anything automatically.",
        "",
        "## Start here",
        "",
        f'<a id="{lead.highlight_anchor}"></a>',
        '<p align="center">',
        f'  <a href="{lead.filename}"><img src="{lead.image}" '
        f'alt="{lead.title} running in AVL BASIC" width="820"></a>',
        "</p>",
        '<p align="center">',
        f"  <strong>{lead.featured_order}. {lead.title}</strong><br>",
        f"  {lead.description}<br>",
        f"  <sub><strong>Shows:</strong> {lead.techniques.replace(';', ' · ')}</sub><br>",
        f'  <code>RUN "/samples/{lead.filename}"</code>',
        "</p>",
        "",
        "<table>",
    ]

    remaining = featured[1:]
    for index in range(0, len(remaining), 2):
        lines.append("<tr>")
        lines.extend(markdown_card(remaining[index]))
        if index + 1 < len(remaining):
            lines.extend(markdown_card(remaining[index + 1]))
        else:
            lines.append('<td width="50%"></td>')
        lines.append("</tr>")
    lines.extend(
        [
            "</table>",
            "",
            "Every image above was captured from the real Rust runtime. The capture tool",
            "uses temporary instrumented copies, so the original BASIC programs remain",
            "untouched:",
            "",
            "```console",
            "python tools/generate_showcase.py",
            "```",
            "",
            "## Suggested learning routes",
            "",
            "- **From wireframes to texture mapping:** `g-cube2.bas` → `g-cube-tquad.bas`.",
            "- **Build a texture mapper, then use the native primitive:** `g-zoomer.bas` →",
            "  `g-zoomer-tquad.bas`.",
            "- **From flat to interpolated light:** `g-lambert.bas` → `g-gouraud.bas`.",
            "- **Build a demoscene effect:** `g-starfield.bas` + `g-sine-scroll.bas` →",
            "  `g-old-school.bas`.",
            "- **Compare two tunnel engines:** `g-tunnel.bas` → `g-tunnel-tquad.bas`.",
            "- **Learn collisions, then build a game:** `g-balls.bas` + `g-sprite5.bas` →",
            "  `g-arkanoid.bas`.",
            "- **Modernize a classic algorithm:** `pimachin.bas` → `pimachin-modern.bas`.",
            "",
            f"## Full catalog — {len(samples)} programs",
            "",
            "The catalog separates polished pieces from small, purposeful probes. That",
            "makes the latter easier to find without pretending every test is a headline",
            "demo.",
            "",
        ]
    )

    for category in categories:
        category_samples = [sample for sample in samples if sample.category == category]
        lines.extend(
            [
                f"### {category} ({len(category_samples)})",
                "",
                "| Sample | What it demonstrates | Techniques |",
                "|---|---|---|",
            ]
        )
        for sample in category_samples:
            techniques = sample.techniques.replace(";", ", ")
            star = (
                f" [★](#{sample.highlight_anchor})"
                if sample.featured_order is not None
                else ""
            )
            lines.append(
                f'| <a id="{sample.anchor}"></a>'
                f"[`{sample.filename}`]({sample.filename}){star} | "
                f"{markdown_escape(sample.description)} | "
                f"{markdown_escape(techniques)} |"
            )
        lines.append("")

    lines.extend(
        [
            f"★ Included in the {EXPECTED_FEATURED}-example tour.",
            "",
            "## Run from a shell",
            "",
            "From the repository or package root:",
            "",
            "```console",
            "avl-basic samples/g-old-school.bas",
            "```",
            "",
            "On Windows, use `avl-basic.exe` and backslashes if preferred.",
            "",
        ]
    )
    return "\n".join(lines)


def html_techniques(sample: Sample) -> str:
    return "".join(
        f"<li>{escape(technique)}</li>" for technique in sample.technique_list
    )


def html_showcase_card(sample: Sample, *, lead: bool = False) -> list[str]:
    filename = escape(sample.filename, quote=True)
    image = escape(sample.image, quote=True)
    title = escape(sample.title)
    description = escape(sample.description)
    command = escape(f'RUN "/samples/{sample.filename}"')
    classes = "showcase-card lead" if lead else "showcase-card"
    loading = "eager" if lead else "lazy"
    return [
        f'<article class="{classes}" id="{sample.highlight_anchor}">',
        f'  <a class="capture" href="{filename}" aria-label="Open {title}">',
        f'    <img src="{image}" alt="{title} running in AVL BASIC" '
        f'loading="{loading}">',
        f'    <span class="number">{sample.featured_order:02d}</span>',
        "  </a>",
        '  <div class="card-copy">',
        '    <p class="eyebrow">Featured program</p>',
        f"    <h3>{title}</h3>",
        f"    <p>{description}</p>",
        f'    <ul class="techniques" aria-label="Techniques">{html_techniques(sample)}</ul>',
        '    <div class="program-actions">',
        f'      <a class="open-program" href="{filename}">Open {filename}</a>',
        f"      <code>{command}</code>",
        "    </div>",
        "  </div>",
        "</article>",
    ]


def render_html(samples: list[Sample]) -> str:
    featured = featured_samples(samples)
    categories = category_names(samples)
    lines = [
        "<!doctype html>",
        '<html lang="en">',
        "<head>",
        '  <meta charset="utf-8">',
        '  <meta name="viewport" content="width=device-width, initial-scale=1">',
        '  <meta name="color-scheme" content="dark">',
        '  <link rel="icon" href="data:,">',
        "  <title>AVL BASIC — 115 runnable sample programs</title>",
        "  <style>",
        "    :root {",
        "      color-scheme: dark;",
        "      --ink: #f6f7fb;",
        "      --muted: #adb5c9;",
        "      --panel: rgba(17, 22, 38, 0.88);",
        "      --line: rgba(255, 255, 255, 0.12);",
        "      --cyan: #45ddff;",
        "      --amber: #ffd166;",
        "      --pink: #ff5cab;",
        "      --shadow: 0 24px 70px rgba(0, 0, 0, 0.42);",
        "    }",
        "    * { box-sizing: border-box; }",
        "    html { scroll-behavior: smooth; }",
        "    body {",
        "      margin: 0;",
        "      color: var(--ink);",
        "      background:",
        "        radial-gradient(circle at 12% 0%, rgba(69, 221, 255, 0.14), transparent 31rem),",
        "        radial-gradient(circle at 92% 15%, rgba(255, 92, 171, 0.11), transparent 32rem),",
        "        #080b13;",
        "      font: 16px/1.6 Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif;",
        "    }",
        "    a { color: var(--cyan); }",
        "    a:hover { color: #a7efff; }",
        "    img { display: block; max-width: 100%; }",
        "    code { font-family: \"Cascadia Code\", \"SFMono-Regular\", Consolas, monospace; font-size: 0.84rem; }",
        "    .page { width: min(1220px, calc(100% - 36px)); margin: 0 auto; }",
        "    .hero { padding: 84px 0 58px; }",
        "    .kicker, .eyebrow {",
        "      margin: 0 0 8px; color: var(--cyan); font-size: 0.75rem; font-weight: 800;",
        "      letter-spacing: 0.14em; text-transform: uppercase;",
        "    }",
        "    h1, h2, h3 { line-height: 1.08; margin-top: 0; }",
        "    h1 { max-width: 900px; margin-bottom: 22px; font-size: clamp(3.1rem, 9vw, 7.4rem); letter-spacing: -0.065em; }",
        "    h1 span { color: var(--amber); }",
        "    .hero-copy { max-width: 770px; color: #ccd2df; font-size: 1.18rem; }",
        "    .stats { display: flex; flex-wrap: wrap; gap: 10px; margin: 30px 0; padding: 0; list-style: none; }",
        "    .stats li, .category-nav a {",
        "      border: 1px solid var(--line); border-radius: 999px; background: rgba(255, 255, 255, 0.045);",
        "      padding: 7px 13px; color: #dce1eb; text-decoration: none;",
        "    }",
        "    .quick-start {",
        "      display: inline-block; max-width: 100%; margin: 0; border: 1px solid rgba(69, 221, 255, 0.3);",
        "      border-radius: 14px; background: #050711; padding: 16px 20px; overflow-x: auto;",
        "      color: #d9f8ff; box-shadow: var(--shadow);",
        "    }",
        "    .section-heading { display: flex; justify-content: space-between; align-items: end; gap: 24px; margin: 30px 0 24px; }",
        "    .section-heading h2 { margin-bottom: 0; font-size: clamp(2rem, 5vw, 4rem); letter-spacing: -0.045em; }",
        "    .section-heading p { max-width: 520px; margin: 0; color: var(--muted); }",
        "    .showcase-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 22px; }",
        "    .showcase-card {",
        "      min-width: 0; overflow: hidden; border: 1px solid var(--line); border-radius: 20px;",
        "      background: var(--panel); box-shadow: 0 14px 44px rgba(0, 0, 0, 0.25);",
        "    }",
        "    .showcase-card.lead { grid-column: 1 / -1; display: grid; grid-template-columns: minmax(0, 1.45fr) minmax(310px, 0.55fr); }",
        "    .capture { position: relative; display: block; overflow: hidden; background: #000; }",
        "    .capture img { width: 100%; height: auto; aspect-ratio: 4 / 3; object-fit: contain; }",
        "    .lead .capture img { height: 100%; min-height: 430px; object-fit: contain; }",
        "    .number {",
        "      position: absolute; top: 14px; left: 14px; display: grid; width: 42px; height: 42px;",
        "      place-items: center; border: 1px solid rgba(255, 255, 255, 0.32); border-radius: 50%;",
        "      background: rgba(4, 6, 13, 0.82); color: var(--amber);",
        "      font: 800 0.8rem/1 ui-monospace, monospace; backdrop-filter: blur(8px);",
        "    }",
        "    .card-copy { padding: 23px 24px 25px; }",
        "    .card-copy h3 { margin-bottom: 10px; font-size: clamp(1.35rem, 3vw, 2.1rem); letter-spacing: -0.025em; }",
        "    .card-copy > p:not(.eyebrow) { margin: 0; color: #c7cddd; }",
        "    .techniques { display: flex; flex-wrap: wrap; gap: 7px; margin: 18px 0 20px; padding: 0; list-style: none; }",
        "    .techniques li {",
        "      border: 1px solid rgba(255, 209, 102, 0.22); border-radius: 999px;",
        "      background: rgba(255, 209, 102, 0.07); padding: 3px 9px; color: #f6dda4; font-size: 0.76rem;",
        "    }",
        "    .program-actions { display: grid; gap: 10px; }",
        "    .open-program { font-weight: 750; text-decoration: none; }",
        "    .program-actions code {",
        "      display: block; max-width: 100%; overflow-x: auto; border-radius: 8px;",
        "      background: #080b13; padding: 9px 11px; color: #d8f8ff; white-space: nowrap;",
        "    }",
        "    .catalog { margin-top: 90px; padding-bottom: 70px; }",
        "    .category-nav { display: flex; flex-wrap: wrap; gap: 8px; margin: 26px 0 36px; }",
        "    .category-nav a { font-size: 0.84rem; }",
        "    .catalog-group {",
        "      margin: 18px 0; border: 1px solid var(--line); border-radius: 16px;",
        "      background: rgba(13, 17, 29, 0.82); overflow: hidden;",
        "    }",
        "    .catalog-group h3 { margin: 0; padding: 20px 22px; font-size: 1.25rem; }",
        "    .table-wrap { overflow-x: auto; }",
        "    table { width: 100%; border-collapse: collapse; }",
        "    th, td { border-top: 1px solid var(--line); padding: 12px 16px; text-align: left; vertical-align: top; }",
        "    th { color: var(--muted); font-size: 0.72rem; letter-spacing: 0.09em; text-transform: uppercase; }",
        "    td:first-child { min-width: 190px; }",
        "    td:last-child { color: var(--muted); }",
        "    tr:target { background: rgba(69, 221, 255, 0.09); }",
        "    .featured-mark { color: var(--amber); text-decoration: none; }",
        "    footer { border-top: 1px solid var(--line); padding: 28px 0 50px; color: var(--muted); }",
        "    .back-top { float: right; text-decoration: none; }",
        "    @media (max-width: 780px) {",
        "      .page { width: min(100% - 22px, 1220px); }",
        "      .hero { padding-top: 54px; }",
        "      .section-heading { display: block; }",
        "      .section-heading p { margin-top: 12px; }",
        "      .showcase-grid { grid-template-columns: 1fr; }",
        "      .showcase-card.lead { display: block; grid-column: auto; }",
        "      .lead .capture img { min-height: 0; object-fit: contain; }",
        "      .card-copy { padding: 19px; }",
        "    }",
        "    @media print {",
        "      body { background: #fff; color: #111; }",
        "      .page { width: 100%; }",
        "      .hero { padding-top: 20px; }",
        "      .showcase-card, .catalog-group { break-inside: avoid; box-shadow: none; background: #fff; color: #111; }",
        "      .card-copy > p:not(.eyebrow), td:last-child, footer { color: #333; }",
        "    }",
        "  </style>",
        "</head>",
        "<body>",
        '<header class="hero" id="top">',
        '  <div class="page">',
        '    <p class="kicker">Native Rust runtime · classic BASIC immediacy</p>',
        "    <h1>Small programs.<br><span>Serious range.</span></h1>",
        '    <p class="hero-copy">AVL BASIC includes complete visual pieces, playable programs, numerical algorithms, interactive explorers, and focused teaching examples. Every capture below comes from a real program running in the interpreter.</p>',
        '    <ul class="stats">',
        f"      <li>{len(featured)} highlights</li>",
        f"      <li>{len(samples)} runnable programs</li>",
        f"      <li>{len(categories)} categories</li>",
        "      <li>Works offline</li>",
        "    </ul>",
        '    <pre class="quick-start"><code>TOUR\nSAMPLES\nRUN &quot;/samples/g-old-school.bas&quot;</code></pre>',
        "  </div>",
        "</header>",
        "<main>",
        '  <section class="page" aria-labelledby="highlights-heading">',
        '    <div class="section-heading">',
        '      <h2 id="highlights-heading">The visual tour</h2>',
        f'      <p>{EXPECTED_FEATURED} programs chosen for impact, range, and teaching value. Open any source file directly or copy its ready-to-run command.</p>',
        "    </div>",
        '    <div class="showcase-grid">',
    ]
    for index, sample in enumerate(featured):
        lines.extend(f"      {line}" for line in html_showcase_card(sample, lead=index == 0))
    lines.extend(
        [
            "    </div>",
            "  </section>",
            '  <section class="page catalog" aria-labelledby="catalog-heading">',
            '    <div class="section-heading">',
            f'      <h2 id="catalog-heading">All {len(samples)} programs</h2>',
            '      <p>The complete collection, including polished pieces, compact lessons, benchmarks, and purposeful probes. A star links back to programs in the visual tour.</p>',
            "    </div>",
            '    <nav class="category-nav" aria-label="Sample categories">',
        ]
    )
    for index, category in enumerate(categories, start=1):
        lines.append(f'      <a href="#category-{index:02d}">{escape(category)}</a>')
    lines.append("    </nav>")

    for category_index, category in enumerate(categories, start=1):
        category_samples = [sample for sample in samples if sample.category == category]
        lines.extend(
            [
                f'    <section class="catalog-group" id="category-{category_index:02d}">',
                f"      <h3>{escape(category)} <small>({len(category_samples)})</small></h3>",
                '      <div class="table-wrap">',
                "        <table>",
                "          <thead><tr><th>Program</th><th>What it demonstrates</th><th>Techniques</th></tr></thead>",
                "          <tbody>",
            ]
        )
        for sample in category_samples:
            filename = escape(sample.filename, quote=True)
            star = ""
            if sample.featured_order is not None:
                star = (
                    f' <a class="featured-mark" href="#{sample.highlight_anchor}" '
                    f'title="Highlight {sample.featured_order}">★</a>'
                )
            lines.extend(
                [
                    f'            <tr id="{sample.anchor}">',
                    f'              <td><a href="{filename}"><code>{filename}</code></a>{star}</td>',
                    f"              <td>{escape(sample.description)}</td>",
                    f"              <td>{escape(sample.techniques.replace(';', ', '))}</td>",
                    "            </tr>",
                ]
            )
        lines.extend(
            [
                "          </tbody>",
                "        </table>",
                "      </div>",
                "    </section>",
            ]
        )
    lines.extend(
        [
            "  </section>",
            "</main>",
            "<footer>",
            '  <div class="page">',
            '    <a class="back-top" href="#top">Back to top ↑</a>',
            "    Generated deterministically from <code>samples/catalog.tsv</code>.",
            "  </div>",
            "</footer>",
            "</body>",
            "</html>",
            "",
        ]
    )
    return "\n".join(lines)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail if samples/README.md or samples/index.html is not up to date",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    samples = read_catalog()
    validate(samples)
    outputs = {
        MARKDOWN_OUTPUT: render_markdown(samples),
        HTML_OUTPUT: render_html(samples),
    }
    if args.check:
        stale = [
            path.relative_to(ROOT)
            for path, rendered in outputs.items()
            if not path.is_file() or path.read_text(encoding="utf-8") != rendered
        ]
        if stale:
            joined = ", ".join(str(path) for path in stale)
            raise SystemExit(
                f"Generated showcase documentation is stale ({joined}); "
                "run python tools/render_showcase_docs.py"
            )
        names = " and ".join(str(path.relative_to(ROOT)) for path in outputs)
        print(f"verified {names} and {len(samples)} catalog entries")
        return 0
    for path, rendered in outputs.items():
        path.write_text(rendered, encoding="utf-8", newline="\n")
        print(f"rendered {path.relative_to(ROOT)} from {len(samples)} catalog entries")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
