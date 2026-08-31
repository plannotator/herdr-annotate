#!/usr/bin/env python3
"""Structural assertions for the distributed Full-mode manifest."""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path


PROGRAM = "./bin/plannotator-tui.exe"
EXPECTED_COMMANDS = {
    ("panes", "doc"): [PROGRAM, "herdr", "pane"],
    ("actions", "open"): [PROGRAM, "herdr", "open"],
    ("actions", "open-link"): [PROGRAM, "herdr", "open"],
    ("actions", "last"): [PROGRAM, "herdr", "last"],
}
UNIX_BUILD = ["bash", "scripts/fetch-plannotator-tui.sh"]
WINDOWS_BUILD = [
    "powershell.exe",
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    "scripts/fetch-plannotator-tui.ps1",
]


def fail(message: str) -> None:
    raise AssertionError(message)


def platforms(manifest: dict[str, object], item: dict[str, object]) -> set[str]:
    value = item.get("platforms", manifest.get("platforms", []))
    if not isinstance(value, list) or not all(isinstance(entry, str) for entry in value):
        fail(f"invalid platforms: {value!r}")
    return set(value)


def effective_entry(
    manifest: dict[str, object], table: str, entry_id: str
) -> dict[str, object]:
    items = manifest.get(table, [])
    if not isinstance(items, list):
        fail(f"[[{table}]] is not an array")
    matches = [
        item
        for item in items
        if isinstance(item, dict)
        and item.get("id") == entry_id
        and "windows" in platforms(manifest, item)
    ]
    if len(matches) != 1:
        fail(f"expected one effective Windows {table}.{entry_id}, found {len(matches)}")
    return matches[0]


def main() -> None:
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent
    manifest_path = root / "herdr-plugin.toml"
    with manifest_path.open("rb") as handle:
        manifest = tomllib.load(handle)

    if "windows" not in platforms(manifest, {}):
        fail("top-level manifest does not include Windows")
    if (root / "plannotator-tui.version").read_text(encoding="utf-8").strip() != "0.6.0":
        fail("plannotator-tui.version is not 0.6.0")

    builds = manifest.get("build", [])
    if not isinstance(builds, list):
        fail("[[build]] is not an array")
    windows_builds = [
        item
        for item in builds
        if isinstance(item, dict) and "windows" in platforms(manifest, item)
    ]
    if len(windows_builds) != 1:
        fail(f"expected one effective Windows Full build, found {len(windows_builds)}")
    if windows_builds[0].get("command") != WINDOWS_BUILD:
        fail(f"unexpected Windows build argv: {windows_builds[0].get('command')!r}")
    unix_builds = [
        item
        for item in builds
        if isinstance(item, dict) and platforms(manifest, item) == {"macos", "linux"}
    ]
    if len(unix_builds) != 1 or unix_builds[0].get("command") != UNIX_BUILD:
        fail(f"unexpected Unix build entries: {unix_builds!r}")

    for (table, entry_id), expected in EXPECTED_COMMANDS.items():
        command = effective_entry(manifest, table, entry_id).get("command")
        if command != expected:
            fail(f"unexpected {table}.{entry_id} argv: {command!r}")
        forbidden = {"sh", "bash", "-c"}
        if any(
            not isinstance(argument, str)
            or argument.lower() in forbidden
            or argument.lower().endswith(".sh")
            or "$" in argument
            for argument in command
        ):
            fail(f"shell or interpolation found in {table}.{entry_id}: {command!r}")

    handler = effective_entry(manifest, "link_handlers", "markdown-file")
    if handler.get("action") != "open-link":
        fail(f"markdown-file points to {handler.get('action')!r}")


if __name__ == "__main__":
    main()
