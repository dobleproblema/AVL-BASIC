"""Build a standalone package README without changing the GitHub Markdown source.

Requires Python 3.10+ and `python -m pip install -r tools/requirements-docs.txt`.
Writes README.html to dist/ by default; --output selects the package directory.
"""
from __future__ import annotations

import argparse
import base64
import html
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import quote, unquote, urlsplit, urlunsplit

ROOT = Path(__file__).resolve().parents[1]
GITHUB = "https://github.com/dobleproblema/AVL-BASIC"
IMAGE_TYPES = {".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg",
               ".gif": "image/gif", ".webp": "image/webp", ".svg": "image/svg+xml"}
CSS = """
:root { color-scheme: light dark; --page:#f4f6f7; --surface:#fff; --ink:#242c32;
  --muted:#55636b; --accent:#11666a; --line:#dce3e6; --inline:#edf2f3; }
@media (prefers-color-scheme:dark) {
  :root { --page:#101518; --surface:#192126; --ink:#e4ebed; --muted:#afbdc5;
    --accent:#79d6cd; --line:#35434c; --inline:#26333b; }
}
* { box-sizing:border-box; }
html { scroll-padding-top:1rem; }
body { margin:0; background:var(--page); color:var(--ink);
  font:16px/1.7 system-ui,-apple-system,"Segoe UI",sans-serif; }
.page { max-width:1060px; margin:32px auto; padding:24px 48px 40px;
  background:var(--surface); border:1px solid var(--line); border-radius:12px; }
.navigation { display:flex; flex-wrap:wrap; gap:8px 24px; padding-bottom:20px;
  border-bottom:1px solid var(--line); font-size:.9rem; }
a { color:var(--accent); text-underline-offset:3px; overflow-wrap:anywhere; }
a:hover { text-decoration-thickness:2px; }
a:focus-visible { outline:3px solid var(--accent); outline-offset:4px; }
h1,h2,h3 { line-height:1.25; letter-spacing:-.025em; }
h1 { font-size:2.6rem; margin:26px 0; }
h2 { font-size:1.65rem; margin:2.2em 0 .8em; padding-bottom:.4em;
  border-bottom:1px solid var(--line); }
p { margin:1em 0; }
li { margin:.35em 0; }
ul,ol { padding-left:1.5em; }
img { display:block; max-width:100%; height:auto; border-radius:6px; }
p[align=center] img { margin:0 auto; }
code { font-family:ui-monospace,Consolas,"Liberation Mono",monospace; font-size:.9em;
  padding:.12em .3em; background:var(--inline); border-radius:4px;
  overflow-wrap:anywhere; }
pre { padding:18px 22px; background:#000; color:#ededed; border-radius:8px;
  overflow-x:auto; line-height:1.55; }
pre code { padding:0; background:transparent; color:inherit; overflow-wrap:normal; }
table { border-collapse:separate; border-spacing:12px; width:100%; margin:1.5em 0; }
td { padding:12px; vertical-align:top; background:var(--inline); border-radius:8px;
  font-size:.9rem; line-height:1.5; }
td img { width:100%; }
td br { display:none; }
td strong { display:block; margin:12px 0 6px; line-height:1.3; }
footer { margin-top:3em; padding-top:1em; border-top:1px solid var(--line);
  font-size:.85rem; color:var(--muted); }
@media (max-width:680px) {
  .page { margin:0; padding:18px 20px 28px; border:0; border-radius:0; }
  h1 { font-size:2.1rem; } h2 { font-size:1.4rem; }
  table,tbody,tr,td { display:block; width:100%; }
  table { border-spacing:0; } td { margin:14px 0; }
  pre { padding:14px; }
}
@media print {
  :root { --page:#fff; --surface:#fff; --ink:#000; --muted:#333;
    --accent:#000; --line:#aaa; --inline:#eee; }
  .page { max-width:none; margin:0; padding:0; border:0; }
  .navigation,footer { display:none; }
  h1,h2,h3 { break-after:avoid; } img,pre,tr { break-inside:avoid; }
  pre { white-space:pre-wrap; overflow-wrap:anywhere; background:#eee; color:#000; }
}
"""


def source_path(url: str) -> Path:
    path = (ROOT / unquote(urlsplit(url).path)).resolve()
    if not path.is_relative_to(ROOT) or not path.exists():
        raise ValueError(f"Missing or outside-repository README resource: {url}")
    return path


def package_link(href: str) -> str:
    url = urlsplit(href)
    if url.scheme or url.netloc or not url.path:
        return href
    if url.path in {"MANUAL.html", "MANUAL.es.html", "README.html"}:
        return href
    path = source_path(href)
    relative = path.relative_to(ROOT).as_posix()
    if relative == "README.md":
        return urlunsplit(("", "", "README.html", url.query, url.fragment))
    # Keep bundled examples and licenses usable without an Internet connection.
    # Markdown catalogs and source-only paths need GitHub's rendered/source view.
    if relative in {"COPYING", "LICENSES"} or (
        (relative == "samples" or relative.startswith("samples/"))
        and path.suffix.lower() != ".md"
    ):
        return href
    kind = "tree" if path.is_dir() else "blob"
    return urlunsplit(("https", "github.com",
                      f"/dobleproblema/AVL-BASIC/{kind}/main/{quote(relative, safe='/')}",
                      url.query, url.fragment))


class EmbedResources(HTMLParser):
    """Transform rendered HTML, including the raw HTML image gallery in README.md."""

    def __init__(self):
        super().__init__(convert_charrefs=False)
        self.parts: list[str] = []
        self.images = 0

    def handle_starttag(self, tag, attrs):
        values = dict(attrs)
        if tag == "img":
            url = urlsplit(values.get("src", ""))
            if url.scheme or url.netloc or not url.path or "srcset" in values:
                raise ValueError("README images must be local files without srcset")
            path = source_path(values["src"])
            mime = IMAGE_TYPES.get(path.suffix.lower())
            if not mime:
                raise ValueError(f"Unsupported README image type: {path}")
            values["src"] = f"data:{mime};base64," + base64.b64encode(path.read_bytes()).decode("ascii")
            self.images += 1
        elif tag == "a" and "href" in values:
            values["href"] = package_link(values["href"])
        attributes = "".join(f' {key}="{html.escape(value, quote=True)}"' if value is not None
                             else f" {key}" for key, value in values.items())
        self.parts.append(f"<{tag}{attributes}>")

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)

    def handle_endtag(self, tag):
        self.parts.append(f"</{tag}>")

    def handle_data(self, data):
        self.parts.append(data)

    def handle_entityref(self, name):
        self.parts.append(f"&{name};")

    def handle_charref(self, name):
        self.parts.append(f"&#{name};")

    def handle_comment(self, data):
        self.parts.append(f"<!--{data}-->")


def render() -> tuple[str, int]:
    try:
        import markdown
    except ImportError:
        raise SystemExit("Install the README build dependency first: "
                         "python -m pip install -r tools/requirements-docs.txt") from None
    body = markdown.markdown((ROOT / "README.md").read_text(encoding="utf-8"),
                             extensions=["fenced_code", "tables", "toc"], output_format="html")
    embedded = EmbedResources()
    embedded.feed(body)
    embedded.close()
    document = f'''<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<title>AVL BASIC — README</title>
<style>{CSS}</style>
</head>
<body>
<div class="page">
<nav class="navigation" aria-label="Project documentation">
<a href="MANUAL.html">English manual</a>
<a href="MANUAL.es.html" lang="es">Manual en español</a>
<a href="{GITHUB}">Project on GitHub</a>
</nav>
<main>{''.join(embedded.parts)}</main>
<footer>AVL BASIC · Project overview and image gallery · Readable offline</footer>
</div>
</body>
</html>
'''
    return document, embedded.images


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    parser.add_argument("--check", action="store_true", help="Check generated README without changing it")
    args = parser.parse_args()
    document, images = render()
    destination = args.output / "README.html"
    data = document.encode("utf-8")
    if args.check:
        if not destination.is_file() or destination.read_bytes() != data:
            raise SystemExit(f"Stale or missing generated file: {destination}")
    else:
        args.output.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
    print(f"{'Checked' if args.check else 'Generated'} {destination} "
          f"({images} embedded images, {len(data):,} bytes)")


if __name__ == "__main__":
    main()
