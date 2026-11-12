#!/usr/bin/env python3
"""Size of the built web app against the budget (docs/architecture.md §7).

  python3 tools/web_size.py        # after `npm run build`; exit 1 when over budget

The shell is every JS file `web/build/index.html` loads before the first screen: its
`<link rel="modulepreload">` tags and the `import("…")` calls of its inline script. Each file is
compressed with gzip at level 6, as a server would send it, and the sizes are summed. Also printed:
the largest route nodes' own sizes, all JS together and the WebAssembly module of the query language
(step 1.5d) when the build has one.
"""

import gzip
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUILD = ROOT / "web" / "build"
SHELL_BUDGET = 150 * 1024
WASM_BUDGET = 120 * 1024

PRELOAD_RE = re.compile(r'<link\s+href="([^"]+\.js)"\s+rel="modulepreload"')
IMPORT_RE = re.compile(r'import\("([^"]+\.js)"\)')


def gz(path):
    return len(gzip.compress(path.read_bytes(), compresslevel=6))


def kb(size):
    return f"{size / 1024:.1f} KB"


def main():
    index = BUILD / "index.html"
    if not index.exists():
        print("web/build/index.html is missing: run `npm run build` first", file=sys.stderr)
        return 1
    html = index.read_text(encoding="utf-8")
    shell = []
    for url in PRELOAD_RE.findall(html) + IMPORT_RE.findall(html):
        if url not in shell:
            shell.append(url)
    missing = [url for url in shell if not (BUILD / url.lstrip("/")).exists()]
    if not shell or missing:
        print(f"cannot read the shell from index.html (missing: {missing})", file=sys.stderr)
        return 1
    shell_size = sum(gz(BUILD / url.lstrip("/")) for url in shell)

    scripts = sorted(BUILD.glob("_app/**/*.js"))
    total = sum(gz(path) for path in scripts)
    nodes = sorted(((gz(p), p) for p in BUILD.glob("_app/immutable/nodes/*.js")), reverse=True)
    wasm = sorted(BUILD.glob("_app/**/*.wasm"))

    ok = shell_size <= SHELL_BUDGET
    print(f"Shell JS: {kb(shell_size)} gzip in {len(shell)} files (budget {kb(SHELL_BUDGET)})")
    print(f"All JS: {kb(total)} gzip in {len(scripts)} files")
    print(
        f"Largest of {len(nodes)} route nodes (own size, gzip): "
        + ", ".join(f"node {p.name.split('.')[0]} {kb(size)}" for size, p in nodes[:5])
    )
    for path in wasm:
        size = gz(path)
        within = size <= WASM_BUDGET
        ok &= within
        print(f"WebAssembly {path.name}: {kb(size)} gzip (budget {kb(WASM_BUDGET)})")
        if not within:
            print("  ^ over budget")
    if shell_size > SHELL_BUDGET:
        print("  ^ the shell is over budget")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
