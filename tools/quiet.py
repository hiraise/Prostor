#!/usr/bin/env python3
"""Run commands with short output (D-039).

  tools/quiet.py --name check 'fmt: cargo fmt --check' 'clippy: cargo clippy' ...
      Runs the stages in order and prints one line per stage: its time and the result
      line of a test runner. The full output goes to target/logs/<name>.log. The first
      failing stage stops the run and prints only its errors (with the log line to read
      on from). Warnings of a passing stage are printed too.
  ... | tools/quiet.py --filter
      Copies the input without build progress lines (cargo, docker compose).

V=1 in the environment streams the full output instead (for people watching a run).
"""

import argparse
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Build progress: says nothing about the result.
NOISE = re.compile(
    r"^\s*(Compiling|Checking|Finished|Running|Fresh|Blocking|Downloaded|Downloading|"
    r"Updating|Locking|Adding|Removing|Packaging|Documenting)\s"
    r"|^\s*(Container|Network|Volume)\s\S+\s+(Running|Waiting|Healthy|Started|Starting|"
    r"Created|Creating|Stopped|Stopping|Removed|Removing)\b"
)
# The result line of each runner; the last match is shown after the stage's time.
SUMMARY = [
    re.compile(r"Summary \[[^\]]*\]\s+(.*)"),  # nextest
    re.compile(r"^\s*Tests\s+(\d.*)"),  # vitest
    re.compile(r"^\s*(\d+ (passed|failed|flaky)\b.*)"),  # playwright
    re.compile(r"^(Shell JS: .*)"),  # web_size.py
    re.compile(r"^(\d+ features, \d+ problems)"),  # catalog.py
    re.compile(r"^(\d+ references, \d+ problems)"),  # doc.py check
    re.compile(r"^(Bench time: .*)"),  # bench
]
# Lines that point at a failure; a few lines after each are shown with it.
FAILURE = re.compile(
    r"^(error|warning)(\[\w+\])?:|^\s*-->\s|panicked at|^\s+FAIL\b|^\s*(FAIL|×|✘|❯)\s|"
    r"^\s*\d+\) \[|AssertionError|^\s*(Error|TypeError|ReferenceError):|Expected|Received|"
    r"^\s*(left|right):|^Diff in |^\s*\d+:\d+\s+error\s|^Error\b|ERROR\b|over budget|"
    r"^\s*(Test Files|Tests)\s+.*failed|^\s+\d+ failed|Summary \[|^make(\[\d+\])?: \*\*\*",
)
# Lines that look like failures but are not.
FALSE_FAILURE = re.compile(r"\b0 (ERRORS|errors|failed|problems)\b|COMPLETED .* 0 ERRORS")
WARNING = re.compile(r"^warning(\[\w+\])?:")
# Colors some runners force even into a pipe.
ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")

EXCERPT_LINES = 70
LINE_WIDTH = 240


def short(line):
    line = line.rstrip("\n")
    return line if len(line) <= LINE_WIDTH else line[:LINE_WIDTH] + " …"


def excerpt(lines):
    """Failure lines with a little context; the tail when nothing matches."""
    keep = []
    for n, line in enumerate(lines):
        if FAILURE.search(line) and not FALSE_FAILURE.search(line) and not NOISE.search(line):
            # Rust diagnostics run to a blank line; others get three lines.
            end = n + 1
            limit = n + (14 if line.startswith(("error", "warning")) else 4)
            while end < len(lines) and end < limit and lines[end].strip():
                end += 1
            keep.extend(range(max(0, n - 1) if lines[n - 1].strip() else n, end))
    picked = sorted(set(keep))
    if not picked:
        tail = [l for l in lines if not NOISE.search(l)][-40:]
        return [short(l) for l in tail]
    out, last = [], None
    for n in picked:
        if last is not None and n != last + 1:
            out.append("  …")
        out.append(short(lines[n]))
        last = n
    if len(out) > EXCERPT_LINES:
        out = out[:EXCERPT_LINES] + [f"  … {len(out) - EXCERPT_LINES} more lines in the log"]
    return out


def summary(lines):
    found = None
    for line in lines:
        for rx in SUMMARY:
            m = rx.search(line)
            if m:
                found = m.group(1).strip()
    return found


def run_stages(name, stages):
    log_path = ROOT / "target" / "logs" / f"{name}.log"
    log_path.parent.mkdir(parents=True, exist_ok=True)
    verbose = os.environ.get("V") == "1"
    started = time.monotonic()
    with open(log_path, "w", encoding="utf-8") as log:
        log_line = 1
        for stage in stages:
            label, _, command = stage.partition(":")
            label, command = label.strip(), command.strip()
            header = f"### {label}: {command}\n"
            log.write(header)
            log.flush()
            stage_start_line = log_line + 1
            log_line += 1
            t0 = time.monotonic()
            proc = subprocess.Popen(
                command, shell=True, cwd=ROOT, executable="/bin/bash",
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace",
            )
            lines = []
            for line in proc.stdout:
                line = ANSI.sub("", line)
                lines.append(line)
                log.write(line)
                if verbose:
                    sys.stdout.write(line)
                    sys.stdout.flush()
            code = proc.wait()
            log.flush()
            log_line += len(lines)
            seconds = time.monotonic() - t0
            result = summary(lines)
            status = "ok  " if code == 0 else "FAIL"
            print(f"{status} {label:<14} {seconds:5.0f}s" + (f"  {result}" if result else ""), flush=True)
            if code != 0:
                if not verbose:
                    for line in excerpt(lines):
                        print("  " + line)
                print(f"Stopped at '{label}' (exit {code}). Full log: {log_path.relative_to(ROOT)} "
                      f"from line {stage_start_line}.", flush=True)
                return code
            warnings = [short(l) for l in lines if WARNING.search(l)]
            if warnings and not verbose:
                print(f"  {len(warnings)} warning(s), first ones:")
                for line in warnings[:10]:
                    print("  " + line)
    total = time.monotonic() - started
    print(f"All {len(stages)} stage(s) passed in {total / 60:.1f} min. Log: {log_path.relative_to(ROOT)}")
    return 0


def filter_stream():
    for line in sys.stdin:
        if not NOISE.search(line):
            sys.stdout.write(line)
            sys.stdout.flush()
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--name", default="run", help="log file name under target/logs")
    parser.add_argument("--filter", action="store_true", help="copy stdin without build progress")
    parser.add_argument("stages", nargs="*", help="'label: shell command'")
    args = parser.parse_args()
    if args.filter:
        return filter_stream()
    if not args.stages:
        parser.error("no stages")
    return run_stages(args.name, args.stages)


if __name__ == "__main__":
    sys.exit(main())
