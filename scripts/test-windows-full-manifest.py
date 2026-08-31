#!/usr/bin/env python3
"""Check the gated distributed Full manifest and development parity."""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path


PROGRAM = "./bin/plannotator-tui.exe"
FULL_PLATFORMS = {"macos", "linux"}
UNIX_BUILD = ["bash", "scripts/fetch-plannotator-tui.sh"]
DISTRIBUTED_PANE = [
    "sh",
    "-c",
    'exec bash "$HERDR_PLUGIN_ROOT/scripts/plannotator-tui.sh" herdr pane',
]
DEVELOPMENT_PANE = [
    "sh",
    "-c",
    'exec "$HERDR_PLUGIN_ROOT/bin/plannotator-tui.exe" herdr pane',
]
ACTION_COMMANDS = {
    "open": [PROGRAM, "herdr", "open"],
    "open-link": [PROGRAM, "herdr", "open"],
    "last": [PROGRAM, "herdr", "last"],
}
DEVELOPMENT_BUILDS = [
    ["cargo", "build", "--release", "--manifest-path", "../Cargo.toml"],
    ["bash", "stage-plannotator-tui.sh"],
    [
        "powershell.exe",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        "stage-plannotator-tui.ps1",
    ],
]


def fail(path: Path, message: str) -> None:
    raise AssertionError(f"{path}: {message}")


def load(path: Path) -> dict[str, object]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def platforms(
    path: Path, manifest: dict[str, object], item: dict[str, object]
) -> set[str]:
    value = item.get("platforms", manifest.get("platforms", []))
    if not isinstance(value, list) or not all(isinstance(entry, str) for entry in value):
        fail(path, f"invalid platforms: {value!r}")
    return set(value)


def entry(
    path: Path,
    manifest: dict[str, object],
    table: str,
    entry_id: str,
) -> dict[str, object]:
    entries = manifest.get(table, [])
    if not isinstance(entries, list):
        fail(path, f"[[{table}]] is not an array")
    matches = [
        item
        for item in entries
        if isinstance(item, dict) and item.get("id") == entry_id
    ]
    if len(matches) != 1:
        fail(path, f"expected one {table}.{entry_id}, found {len(matches)}")
    return matches[0]


def builds(path: Path, manifest: dict[str, object]) -> list[dict[str, object]]:
    value = manifest.get("build", [])
    if not isinstance(value, list) or not all(isinstance(item, dict) for item in value):
        fail(path, "[[build]] is not an array of tables")
    return value


def check_top_level_windows(path: Path, manifest: dict[str, object]) -> None:
    if platforms(path, manifest, {}) != {"macos", "linux", "windows"}:
        fail(path, "top-level platforms must be macOS, Linux, and Windows")


def check_distributed(path: Path, version_path: Path) -> None:
    manifest = load(path)
    check_top_level_windows(path, manifest)
    if version_path.read_text(encoding="utf-8").strip() != "0.6.0":
        fail(version_path, "plannotator-tui.version is not 0.6.0")

    build_entries = builds(path, manifest)
    if len(build_entries) != 1:
        fail(path, f"expected one Full build, found {len(build_entries)}")
    build = build_entries[0]
    if platforms(path, manifest, build) != FULL_PLATFORMS:
        fail(path, f"Full build platforms are {platforms(path, manifest, build)!r}")
    if build.get("command") != UNIX_BUILD:
        fail(path, f"unexpected Unix build argv: {build.get('command')!r}")

    pane = entry(path, manifest, "panes", "doc")
    if platforms(path, manifest, pane) != FULL_PLATFORMS:
        fail(path, f"panes.doc platforms are {platforms(path, manifest, pane)!r}")
    if pane.get("command") != DISTRIBUTED_PANE:
        fail(path, f"unexpected panes.doc argv: {pane.get('command')!r}")

    for entry_id, expected in ACTION_COMMANDS.items():
        action = entry(path, manifest, "actions", entry_id)
        if platforms(path, manifest, action) != FULL_PLATFORMS:
            fail(path, f"actions.{entry_id} platforms are not macOS/Linux")
        if action.get("command") != expected:
            fail(path, f"unexpected actions.{entry_id} argv: {action.get('command')!r}")
        if any("sh" in argument.lower() or "$" in argument for argument in expected):
            fail(path, f"shell found in actions.{entry_id}: {expected!r}")

    handler = entry(path, manifest, "link_handlers", "markdown-file")
    if platforms(path, manifest, handler) != FULL_PLATFORMS:
        fail(path, "link_handlers.markdown-file platforms are not macOS/Linux")
    if handler.get("action") != "open-link":
        fail(path, f"markdown-file points to {handler.get('action')!r}")


def check_development(path: Path) -> None:
    manifest = load(path)
    check_top_level_windows(path, manifest)

    build_entries = builds(path, manifest)
    commands = [item.get("command") for item in build_entries]
    if commands != DEVELOPMENT_BUILDS:
        fail(path, f"development build commands are {commands!r}")
    if "windows" not in platforms(path, manifest, build_entries[0]):
        fail(path, "development Cargo build does not run on Windows")
    if platforms(path, manifest, build_entries[1]) != FULL_PLATFORMS:
        fail(path, "development Unix staging platforms differ")
    if platforms(path, manifest, build_entries[2]) != {"windows"}:
        fail(path, "development PowerShell staging is not Windows-only")

    pane = entry(path, manifest, "panes", "doc")
    if platforms(path, manifest, pane) != FULL_PLATFORMS:
        fail(path, "development pane must be limited to macOS/Linux")
    if pane.get("command") != DEVELOPMENT_PANE:
        fail(path, f"unexpected development panes.doc argv: {pane.get('command')!r}")

    for entry_id, expected in ACTION_COMMANDS.items():
        action = entry(path, manifest, "actions", entry_id)
        if "windows" not in platforms(path, manifest, action):
            fail(path, f"development actions.{entry_id} lost Windows support")
        if action.get("command") != expected:
            fail(path, f"development actions.{entry_id} differs: {action.get('command')!r}")

    handler = entry(path, manifest, "link_handlers", "markdown-file")
    if "windows" not in platforms(path, manifest, handler):
        fail(path, "development markdown-file lost Windows support")
    if handler.get("action") != "open-link":
        fail(path, f"development markdown-file points to {handler.get('action')!r}")


def main() -> None:
    if len(sys.argv) > 2:
        raise SystemExit("usage: test-windows-full-manifest.py [development-manifest]")
    root = Path(__file__).resolve().parent.parent
    check_distributed(root / "herdr-plugin.toml", root / "plannotator-tui.version")
    if len(sys.argv) == 2:
        check_development(Path(sys.argv[1]))


if __name__ == "__main__":
    main()
