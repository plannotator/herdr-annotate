#!/usr/bin/env python3
"""Check the gated distributed Full manifest, Windows Full, and development parity."""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path


PROGRAM = "./bin/plannotator-tui.exe"
NATIVE_PROGRAM = "./bin/herdr-annotate.exe"
# Windows Full sits one directory down, so it shares the repository's single native runtime
# the way lite/ does and keeps only plannotator-tui in its own bin/.
WINDOWS_NATIVE_PROGRAM = "../bin/herdr-annotate.exe"
WINDOWS_FULL_BUILDS = [
    [
        "powershell.exe",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        "../scripts/fetch-herdr-annotate.ps1",
    ],
    [
        "powershell.exe",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        "scripts/fetch-plannotator-tui.ps1",
    ],
]
WINDOWS_FULL_PANE = [PROGRAM, "herdr", "pane"]
PANE_SHAPE = {
    "editor": ("Annotate", "popup", 88, 24),
    "manager": ("Annotations", "popup", 100, 30),
}
FULL_PLATFORMS = {"macos", "linux"}
UNIX_BUILD = ["bash", "scripts/fetch-plannotator-tui.sh"]
NATIVE_UNIX_BUILD = ["bash", "scripts/fetch-herdr-annotate.sh"]
NATIVE_WINDOWS_BUILD = [
    "powershell.exe",
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    "scripts/fetch-herdr-annotate.ps1",
]
NATIVE_ENTRIES = {
    "actions": ("capture", "copy-context", "copy-archive", "manage"),
    "panes": ("editor", "manager"),
}
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


def check_native_lite(path: Path, manifest: dict[str, object]) -> None:
    """The Lite tools reach Windows too: one native argv, no shell, no platform gate."""
    for table, identifiers in NATIVE_ENTRIES.items():
        for identifier in identifiers:
            item = entry(path, manifest, table, identifier)
            command = item.get("command")
            if command != [NATIVE_PROGRAM, identifier]:
                fail(path, f"unexpected {table}.{identifier} argv: {command!r}")
            if "platforms" in item:
                fail(path, f"{table}.{identifier} is gated to {item['platforms']!r}")
            if any("$" in argument for argument in command):
                fail(path, f"interpolation found in {table}.{identifier}: {command!r}")


def check_distributed(path: Path, version_path: Path, native_version_path: Path) -> None:
    manifest = load(path)
    check_top_level_windows(path, manifest)
    check_native_lite(path, manifest)
    if version_path.read_text(encoding="utf-8").strip() != "0.9.4":
        fail(version_path, "plannotator-tui.version is not 0.9.4")
    if native_version_path.read_text(encoding="utf-8").strip() != "0.1.0":
        fail(native_version_path, "herdr-annotate.version is not 0.1.0")

    build_entries = builds(path, manifest)
    if len(build_entries) != 3:
        fail(path, f"expected three Full builds, found {len(build_entries)}")
    native_unix, native_windows, build = build_entries
    if platforms(path, manifest, native_unix) != FULL_PLATFORMS:
        fail(path, f"native Unix build platforms are {platforms(path, manifest, native_unix)!r}")
    if native_unix.get("command") != NATIVE_UNIX_BUILD:
        fail(path, f"unexpected native Unix build argv: {native_unix.get('command')!r}")
    if platforms(path, manifest, native_windows) != {"windows"}:
        fail(path, "the native PowerShell build is not Windows-only")
    if native_windows.get("command") != NATIVE_WINDOWS_BUILD:
        fail(path, f"unexpected native Windows build argv: {native_windows.get('command')!r}")
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


def surface(path: Path, manifest: dict[str, object], table: str) -> set[tuple[object, ...]]:
    """Ids, titles, descriptions and contexts, so parity is compared rather than assumed."""
    entries = manifest.get(table, [])
    if not isinstance(entries, list):
        fail(path, f"[[{table}]] is not an array")
    return {
        (
            item.get("id"),
            item.get("title"),
            item.get("description"),
            tuple(item.get("contexts", [])),
        )
        for item in entries
        if isinstance(item, dict)
    }


def check_windows_full(path: Path, root_path: Path) -> None:
    manifest = load(path)
    root = load(root_path)

    if manifest.get("id") != root.get("id") or manifest.get("name") != root.get("name"):
        fail(path, "id and name must match the root manifest")
    if manifest.get("version") != root.get("version"):
        fail(path, f"version {manifest.get('version')!r} differs from the root manifest")
    if platforms(path, manifest, {}) != {"windows"}:
        fail(path, f"top-level platforms are {platforms(path, manifest, {})!r}")
    if manifest.get("min_herdr_version") != "0.9.0":
        fail(path, "Windows Full requires Herdr 0.9.0, where panes resolve relative programs")

    build_entries = builds(path, manifest)
    if [item.get("command") for item in build_entries] != WINDOWS_FULL_BUILDS:
        fail(path, f"unexpected builds: {[i.get('command') for i in build_entries]!r}")
    for item in build_entries:
        # A build gated to macOS/Linux would stage nothing here, leaving the manifest
        # pointing at binaries that were never fetched.
        if "platforms" in item:
            fail(path, f"a build carries a platform gate: {item['platforms']!r}")
        # A pinned command string still has to name a file that exists. The target is
        # resolved from the manifest's own directory, which is the plugin root Herdr runs
        # the build from, rather than from whatever directory this check was started in.
        command = item.get("command", [])
        target = command[command.index("-File") + 1]
        script = (path.parent / target).resolve()
        if not script.is_file():
            fail(path, f"build script {target!r} does not exist at {script}")

    # Parity is the point of the variant: the same surface, reached a different way.
    if surface(path, manifest, "actions") != surface(root_path, root, "actions"):
        fail(path, "action ids, titles, descriptions or contexts differ from the root manifest")

    for table in ("actions", "panes", "link_handlers"):
        for item in manifest.get(table, []):
            identifier = item.get("id")
            # An inherited Unix gate would silently disable the entry on the only platform
            # this variant targets, which is the bug the variant exists to avoid.
            if "platforms" in item:
                fail(path, f"{table}.{identifier} carries a platform gate: {item['platforms']!r}")
            command = item.get("command", [])
            if any("sh" in argument.lower() or "$" in argument for argument in command):
                fail(path, f"shell or interpolation in {table}.{identifier}: {command!r}")

    for identifier in NATIVE_ENTRIES["actions"]:
        command = entry(path, manifest, "actions", identifier).get("command")
        if command != [WINDOWS_NATIVE_PROGRAM, identifier]:
            fail(path, f"actions.{identifier} must reuse the shared runtime: {command!r}")
    for identifier, expected in ACTION_COMMANDS.items():
        command = entry(path, manifest, "actions", identifier).get("command")
        if command != expected:
            fail(path, f"unexpected actions.{identifier} argv: {command!r}")

    panes = manifest.get("panes", [])
    if len(panes) != 3:
        fail(path, f"expected exactly three panes, found {len(panes)}")
    for identifier, (title, placement, width, height) in PANE_SHAPE.items():
        pane = entry(path, manifest, "panes", identifier)
        if (pane.get("title"), pane.get("placement")) != (title, placement):
            fail(path, f"panes.{identifier} shape differs from the root manifest")
        if (pane.get("width"), pane.get("height")) != (width, height):
            fail(path, f"panes.{identifier} is not {width}x{height}")
        if pane.get("command") != [WINDOWS_NATIVE_PROGRAM, identifier]:
            fail(path, f"panes.{identifier} must reuse the shared runtime")

    doc = entry(path, manifest, "panes", "doc")
    if (doc.get("title"), doc.get("placement")) != ("Annotate", "overlay"):
        fail(path, "panes.doc must stay an overlay titled Annotate")
    # No launcher may sit between Herdr and the TUI: a process started through an
    # extended-length path exits immediately unless it is the native binary itself.
    if doc.get("command") != WINDOWS_FULL_PANE:
        fail(path, f"panes.doc must be direct argv, found {doc.get('command')!r}")

    handler = entry(path, manifest, "link_handlers", "markdown-file")
    root_handler = entry(root_path, root, "link_handlers", "markdown-file")
    for key in ("title", "pattern", "action"):
        if handler.get(key) != root_handler.get(key):
            fail(path, f"markdown-file {key} differs from the root manifest")


def main() -> None:
    if len(sys.argv) > 2:
        raise SystemExit("usage: test-windows-full-manifest.py [development-manifest]")
    root = Path(__file__).resolve().parent.parent
    check_distributed(
        root / "herdr-plugin.toml",
        root / "plannotator-tui.version",
        root / "herdr-annotate.version",
    )
    check_windows_full(
        root / "windows-full" / "herdr-plugin.toml",
        root / "herdr-plugin.toml",
    )
    if len(sys.argv) == 2:
        check_development(Path(sys.argv[1]))


if __name__ == "__main__":
    main()
