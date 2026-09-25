#!/usr/bin/env python3
"""Prints the GitHub release description for a Threadrinth version: its
section of CHANGELOG.md plus download hints. Versions without a section
(automatic upstream syncs) get a one-line note instead.

Usage: release-notes.py <version> <upstream version>
"""
import re
import sys
from pathlib import Path

version = sys.argv[1].removeprefix("v")
upstream = sys.argv[2].removeprefix("v")

changelog = (Path(__file__).parent.parent / "CHANGELOG.md").read_text()
section = re.search(
    rf"^## {re.escape(version)}\n(.*?)(?=^## |\Z)", changelog, re.M | re.S
)
changes = (
    section[1].strip()
    if section
    else f"- Updated to Modrinth App {upstream}."
)

print(f"""{changes}

Based on Modrinth App {upstream}.

**Windows:** download the `-setup.exe`. **Linux:** the `.AppImage` (updates itself), or the `.deb` / `.rpm`.""")
