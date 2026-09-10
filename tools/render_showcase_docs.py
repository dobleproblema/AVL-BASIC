"""Render the Markdown gallery and catalog from the sample metadata."""

from __future__ import annotations

import argparse
import csv
from collections import Counter
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SAMPLES = ROOT / "samples"
CATALOG = SAMPLES / "catalog.tsv"
MARKDOWN_OUTPUT = SAMPLES / "README.md"
EXPECTED_SAMPLES = 119
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
        f"AVL BASIC ships with **{len(samples)} runnable programs**. They are not filler or API",
        "snippets: the collection includes complete visual pieces, playable programs,",
        "numerical algorithms, interactive explorers, and focused teaching examples.",
        "",
        f"The {EXPECTED_FEATURED} visual highlights below are followed by the complete",
        f"{len(samples)}-program catalog. On GitHub, select any image or program name",
        "to inspect its BASIC source.",
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
            "- **Save and recover data:** `f-scores.bas` → `f-text.bas` → `f-records.bas`.",
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
            ]
        )
        if category == "Sequential data files":
            lines.extend(
                [
                    "Each example creates or replaces its own named demonstration file beside the",
                    "program, then reads it back.",
                    "",
                ]
            )
        lines.extend(
            [
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
            f"★ Featured in the {EXPECTED_FEATURED}-example gallery.",
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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail if samples/README.md is not up to date",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    samples = read_catalog()
    validate(samples)
    outputs = {MARKDOWN_OUTPUT: render_markdown(samples)}
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
