#!/usr/bin/env python3
"""Prints the next Threadrinth version.

Threadrinth follows the major.minor of the Modrinth release it is based on
(scripts/upstream-version.txt), so upstream code that compares app versions
keeps working, and counts its own patch releases:

  based on Modrinth 0.21.x, last release 0.21.3  ->  0.21.4
  based on Modrinth 0.22.x, last release 0.21.4  ->  0.22.0

Usage: next-version.py <last release tag or empty>
"""
import re
import sys
from pathlib import Path

SEMVER = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)$")

upstream = SEMVER.match(
    (Path(__file__).parent / "upstream-version.txt").read_text().strip()
)
if not upstream:
    sys.exit("scripts/upstream-version.txt must contain a version like v0.21.5")
base = (int(upstream[1]), int(upstream[2]))

last = SEMVER.match(sys.argv[1].strip()) if len(sys.argv) > 1 and sys.argv[1].strip() else None
if last and (int(last[1]), int(last[2])) >= base:
    print(f"{last[1]}.{last[2]}.{int(last[3]) + 1}")
else:
    print(f"{base[0]}.{base[1]}.0")
