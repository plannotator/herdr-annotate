#!/usr/bin/env python3
"""Deterministic golden regression harness for Herdr Annotate Lite.

The goldens under scripts/lite-goldens were recorded from the retired Bun runtime, so a green run
is the standing proof that the native runtime still behaves the way the Bun one did. --record
rewrites them from the current run and is only for a deliberate behavior change.
"""

from __future__ import annotations

import argparse
import base64
import codecs
import copy
import difflib
import fcntl
import json
import os
import pty
import re
import select
import shutil
import signal
import stat
import struct
import subprocess
import sys
import tempfile
import termios
import tomllib
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Iterable, Mapping, Sequence


KNOWN_TIMESTAMPS = {
    "2026-08-08T00:00:00.000Z",
    "2026-08-08T00:00:01.000Z",
    "2026-08-09T10:11:12.000Z",
    "2026-08-09T10:11:13.000Z",
    "2026-08-10T20:21:22.000Z",
    "2026-08-10T20:21:23.000Z",
    "2026-08-20T01:02:03.000Z",
    "2026-08-21T01:02:03.000Z",
}
ISO_PATTERN = re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z")
UUID_PATTERN = re.compile(
    r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b"
)
# The six entrypoints the Lite manifest declares, and the whole surface this harness drives.
ENTRYPOINTS = ("capture", "copy-archive", "copy-context", "editor", "manage", "manager")
# The Full manifest at the repository root starts the binary in its own bin/; the Lite variant
# in lite/ shares that one staged binary, the way it shared the Bun sources.
NATIVE_PROGRAM = "./bin/herdr-annotate.exe"
LITE_NATIVE_PROGRAM = "../bin/herdr-annotate.exe"
PENDING_PATTERN = re.compile(r"pending-\d+-\d+\.json")
TEMP_PATTERN = re.compile(r"\.(annotations|archives)-\d+-\d+\.tmp")
HANDOFF_PATTERN = re.compile(r"herdr-annotate-\d+")
DELIBERATE_DIFFERENCES = ("manager timestamp locale outside en-US",)

# One clipboard operation walks its platform's candidate list until an adapter works. The
# goldens record the role only, so they hold on every platform, and the chain itself is checked
# against this table instead.
CLIPBOARD_CANDIDATES = {
    "darwin": {
        "read": (("pbpaste", ()),),
        "write": (("pbcopy", ()),),
    },
    "linux": {
        "read": (
            ("wl-paste", ("--no-newline",)),
            ("xclip", ("-selection", "clipboard", "-out")),
            ("xsel", ("--clipboard", "--output")),
        ),
        "write": (
            ("wl-copy", ()),
            ("xclip", ("-selection", "clipboard", "-in")),
            ("xsel", ("--clipboard", "--input")),
        ),
    },
}

WIDE_RANGES = (
    (0x1100, 0x115F),
    (0x2E80, 0x303E),
    (0x3041, 0x33FF),
    (0x3400, 0x4DBF),
    (0x4E00, 0x9FFF),
    (0xA000, 0xA4CF),
    (0xA960, 0xA97F),
    (0xAC00, 0xD7A3),
    (0xF900, 0xFAFF),
    (0xFE10, 0xFE19),
    (0xFE30, 0xFE6F),
    (0xFF00, 0xFF60),
    (0xFFE0, 0xFFE6),
    (0x1F300, 0x1F64F),
    (0x1F900, 0x1F9FF),
    (0x20000, 0x3FFFD),
)


def char_width(character: str) -> int:
    code_point = ord(character)
    if code_point < 0x20 or 0x7F <= code_point < 0xA0:
        return 0
    if 0x0300 <= code_point <= 0x036F or 0x200B <= code_point <= 0x200F:
        return 0
    for start, end in WIDE_RANGES:
        if code_point < start:
            return 1
        if code_point <= end:
            return 2
    return 1


@dataclass(frozen=True)
class CommandResult:
    exit_code: int
    stdout: bytes
    stderr: bytes


@dataclass(frozen=True)
class Step:
    label: str
    data: bytes = b""
    coverage: tuple[str, ...] = ()
    process_signal: int | None = None
    # The keystroke ends the process (a manager copy closes the pane). The screen is captured
    # only after the exit has been observed and the output drained, so a slow runner cannot
    # snapshot the frame before the terminal is restored.
    exits: bool = False


@dataclass
class PtyResult:
    screens: list[tuple[str, tuple[tuple[str, ...], ...]]]
    exit_code: int
    osc52: list[str]


class Goldens:
    """Every recorded expectation, and the checks the harness states in its own source."""

    def __init__(self, directory: Path, artifacts: Path, *, record: bool) -> None:
        self.directory = directory
        self.artifacts = artifacts
        self.record = record
        self.groups: dict[str, dict[str, list[str]]] = {}
        self.seen: dict[str, set[str]] = {}
        self.checked = 0
        self.screens = 0
        self.failures: list[str] = []
        self.coverage: set[str] = set()

    @staticmethod
    def _render(value: object, *, screen: bool = False) -> list[str]:
        if screen:
            return ["".join(cell or "·" for cell in row) for row in value]
        if isinstance(value, bytes):
            return value.decode("utf-8", "backslashreplace").split("\n")
        if isinstance(value, str):
            return value.split("\n")
        return json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True).split("\n")

    def _group(self, group: str) -> dict[str, list[str]]:
        if group not in self.groups:
            path = self.directory / f"{group}.json"
            recorded: dict[str, list[str]] = {}
            if not self.record and path.exists():
                recorded = json.loads(path.read_text(encoding="utf-8"))
            self.groups[group] = recorded
            self.seen[group] = set()
        return self.groups[group]

    def check(self, group: str, name: str, value: object, *, screen: bool = False) -> None:
        """Compare one observable with its recording."""
        self.checked += 1
        if screen:
            self.screens += 1
        rendered = self._render(value, screen=screen)
        recorded = self._group(group)
        self.seen[group].add(name)
        if self.record:
            recorded[name] = rendered
            return
        if name not in recorded:
            self.fail(name, f"no golden is recorded in {group}.json\n")
            return
        if recorded[name] != rendered:
            self.fail(name, self._diff(name, recorded[name], rendered, "golden", "actual"))

    def equal(self, name: str, expected: object, actual: object) -> None:
        """Check one expectation this harness states itself, which is never recorded."""
        self.checked += 1
        if expected == actual:
            return
        self.fail(
            name,
            self._diff(name, self._render(expected), self._render(actual), "expected", "actual"),
        )

    @staticmethod
    def _diff(name: str, left: list[str], right: list[str], left_name: str, right_name: str) -> str:
        return "".join(
            difflib.unified_diff(
                [f"{line}\n" for line in left],
                [f"{line}\n" for line in right],
                fromfile=f"{name}.{left_name}",
                tofile=f"{name}.{right_name}",
            )
        )

    def fail(self, name: str, detail: str) -> None:
        self.failures.append(name)
        target = self.artifacts / "failures" / f"{safe_name(name)}.diff"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(detail or f"{name} differs\n", encoding="utf-8")
        print(f"FAILURE {name}\n{detail}", file=sys.stderr)

    def require_coverage(self, required: Iterable[str]) -> None:
        self.equal("screen.key-coverage", [], sorted(set(required) - self.coverage))

    def finish(self) -> None:
        """Write the recording, or report goldens this run never exercised."""
        if self.record:
            self.directory.mkdir(parents=True, exist_ok=True)
            for group, entries in self.groups.items():
                (self.directory / f"{group}.json").write_text(
                    json.dumps(entries, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
                    encoding="utf-8",
                )
            for path in sorted(self.directory.glob("*.json")):
                if path.stem not in self.groups:
                    path.unlink()
            return
        for group, entries in self.groups.items():
            unused = sorted(set(entries) - self.seen[group])
            if unused:
                self.fail(f"{group}.unused-goldens", "\n".join(unused) + "\n")
        unused_groups = sorted(
            path.stem for path in self.directory.glob("*.json") if path.stem not in self.groups
        )
        if unused_groups:
            self.fail("goldens.unused-groups", "\n".join(unused_groups) + "\n")


class TerminalGrid:
    """Small ANSI terminal model for the sequences a Ratatui pane emits."""

    def __init__(self, rows: int, cols: int) -> None:
        self.rows = rows
        self.cols = cols
        self.cells = [[" " for _ in range(cols)] for _ in range(rows)]
        self.row = 0
        self.col = 0
        self.saved_primary: tuple[list[list[str]], int, int] | None = None
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.pending = ""

    def snapshot(self) -> tuple[tuple[str, ...], ...]:
        return tuple(tuple(row) for row in self.cells)

    def text(self) -> str:
        return "\n".join("".join(cell or " " for cell in row) for row in self.cells)

    def clear(self) -> None:
        self.cells = [[" " for _ in range(self.cols)] for _ in range(self.rows)]

    def feed(self, data: bytes) -> None:
        self.pending += self.decoder.decode(data)
        index = 0
        while index < len(self.pending):
            character = self.pending[index]
            if character != "\x1b":
                self._plain(character)
                index += 1
                continue
            if index + 1 >= len(self.pending):
                break
            marker = self.pending[index + 1]
            if marker == "[":
                end = index + 2
                while end < len(self.pending) and not 0x40 <= ord(self.pending[end]) <= 0x7E:
                    end += 1
                if end >= len(self.pending):
                    break
                self._csi(self.pending[index + 2 : end], self.pending[end])
                index = end + 1
                continue
            if marker == "]":
                bell = self.pending.find("\x07", index + 2)
                terminator = self.pending.find("\x1b\\", index + 2)
                candidates = [value for value in (bell, terminator) if value >= 0]
                if not candidates:
                    break
                end = min(candidates)
                index = end + (2 if self.pending.startswith("\x1b\\", end) else 1)
                continue
            if marker in "()" and index + 2 >= len(self.pending):
                break
            index += 3 if marker in "()" else 2
        self.pending = self.pending[index:]

    def _plain(self, character: str) -> None:
        if character == "\r":
            self.col = 0
            return
        if character == "\n":
            self.row = min(self.rows - 1, self.row + 1)
            return
        if character == "\b":
            self.col = max(0, self.col - 1)
            return
        if character == "\t":
            self.col = min(self.cols - 1, ((self.col // 8) + 1) * 8)
            return
        width = char_width(character)
        if width == 0:
            if self.col > 0:
                target = self.col - 1
                if self.cells[self.row][target] == "" and target > 0:
                    target -= 1
                self.cells[self.row][target] += character
            return
        if self.col >= self.cols:
            self.col = 0
            self.row = min(self.rows - 1, self.row + 1)
        self._clear_glyph_at(self.row, self.col)
        self.cells[self.row][self.col] = character
        if width == 2 and self.col + 1 < self.cols:
            self._clear_glyph_at(self.row, self.col + 1)
            self.cells[self.row][self.col + 1] = ""
        self.col += width

    def _clear_glyph_at(self, row: int, column: int) -> None:
        if self.cells[row][column] == "" and column > 0:
            self.cells[row][column - 1] = " "
        if column + 1 < self.cols and self.cells[row][column + 1] == "":
            self.cells[row][column + 1] = " "
        self.cells[row][column] = " "

    @staticmethod
    def _numbers(parameters: str) -> list[int]:
        cleaned = parameters.lstrip("?<>")
        values = cleaned.split(";") if cleaned else [""]
        return [int(value) if value.isdigit() else 0 for value in values]

    def _csi(self, parameters: str, final: str) -> None:
        values = self._numbers(parameters)
        first = values[0] if values else 0
        if final in ("H", "f"):
            self.row = min(self.rows - 1, max(0, (values[0] or 1) - 1))
            column = values[1] if len(values) > 1 else 1
            self.col = min(self.cols - 1, max(0, (column or 1) - 1))
        elif final == "A":
            self.row = max(0, self.row - (first or 1))
        elif final == "B":
            self.row = min(self.rows - 1, self.row + (first or 1))
        elif final == "C":
            self.col = min(self.cols - 1, self.col + (first or 1))
        elif final == "D":
            self.col = max(0, self.col - (first or 1))
        elif final == "G":
            self.col = min(self.cols - 1, max(0, (first or 1) - 1))
        elif final == "d":
            self.row = min(self.rows - 1, max(0, (first or 1) - 1))
        elif final == "J":
            self._erase_display(first)
        elif final == "K":
            self._erase_line(first)
        elif final in ("h", "l") and "1049" in parameters:
            if final == "h":
                self.saved_primary = (copy.deepcopy(self.cells), self.row, self.col)
                self.clear()
                self.row = 0
                self.col = 0
            elif self.saved_primary is not None:
                self.cells, self.row, self.col = self.saved_primary
                self.saved_primary = None

    def _erase_display(self, mode: int) -> None:
        if mode in (2, 3):
            self.clear()
        elif mode == 0:
            for column in range(self.col, self.cols):
                self.cells[self.row][column] = " "
            for row in range(self.row + 1, self.rows):
                self.cells[row] = [" " for _ in range(self.cols)]
        elif mode == 1:
            for row in range(0, self.row):
                self.cells[row] = [" " for _ in range(self.cols)]
            for column in range(0, self.col + 1):
                self.cells[self.row][column] = " "

    def _erase_line(self, mode: int) -> None:
        start, end = (0, self.cols) if mode == 2 else ((0, self.col + 1) if mode == 1 else (self.col, self.cols))
        for column in range(start, end):
            self.cells[self.row][column] = " "


class PtySession:
    def __init__(self, command: Sequence[str], env: Mapping[str, str], cwd: Path, rows: int, cols: int) -> None:
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        self.process = subprocess.Popen(
            list(command),
            cwd=cwd,
            env=dict(env),
            stdin=slave,
            stdout=slave,
            stderr=slave,
            close_fds=True,
            preexec_fn=os.setsid,
        )
        os.close(slave)
        os.set_blocking(master, False)
        self.master = master
        self.grid = TerminalGrid(rows, cols)
        self.raw = bytearray()

    def drain(self, *, quiet: float = 0.08, maximum: float = 2.0) -> None:
        deadline = time.monotonic() + maximum
        quiet_deadline = time.monotonic() + quiet
        while time.monotonic() < deadline:
            timeout = max(0.0, min(quiet_deadline, deadline) - time.monotonic())
            readable, _, _ = select.select([self.master], [], [], timeout)
            if not readable:
                if time.monotonic() >= quiet_deadline:
                    return
                continue
            try:
                data = os.read(self.master, 65536)
            except (BlockingIOError, OSError):
                data = b""
            if not data:
                if self.process.poll() is not None:
                    return
                continue
            self.raw.extend(data)
            self.grid.feed(data)
            quiet_deadline = time.monotonic() + quiet

    def wait_for(self, marker: str, timeout: float = 4.0) -> None:
        deadline = time.monotonic() + timeout
        while marker not in self.grid.text() and time.monotonic() < deadline:
            self.drain(quiet=0.03, maximum=0.2)
        if marker not in self.grid.text():
            raise RuntimeError(f"PTY did not render {marker!r}:\n{self.grid.text()}")

    def send(self, data: bytes) -> None:
        os.write(self.master, data)
        self.drain()

    def wait_exit(self, timeout: float = 5.0) -> None:
        deadline = time.monotonic() + timeout
        while self.process.poll() is None and time.monotonic() < deadline:
            self.drain(quiet=0.05, maximum=0.25)
        if self.process.poll() is None:
            raise RuntimeError("PTY command did not exit after an exiting step")
        # Everything the process wrote on its way out, up to EOF.
        self.drain(quiet=0.1, maximum=1.0)

    def send_signal(self, process_signal: int) -> None:
        os.killpg(self.process.pid, process_signal)
        self.drain()

    def finish(self, timeout: float = 3.0) -> int:
        try:
            code = self.process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(self.process.pid, signal.SIGTERM)
            try:
                code = self.process.wait(timeout=1.0)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid, signal.SIGKILL)
                code = self.process.wait(timeout=1.0)
            raise RuntimeError(f"PTY command did not exit (terminated with {code})")
        self.drain(quiet=0.02, maximum=0.2)
        os.close(self.master)
        return code


OSC52_PATTERN = re.compile(rb"\x1b\]52;([^;]*);([A-Za-z0-9+/=]*)(?:\x07|\x1b\\)")


def osc52_sequences(raw: bytes) -> list[str]:
    """Every OSC 52 clipboard sequence a pane wrote to its terminal, in emission order.

    Sequences are pure ASCII, so they are kept as text and stay readable in a divergence diff.
    """
    return [match.group(0).decode("ascii", "backslashreplace") for match in OSC52_PATTERN.finditer(raw)]


def osc52_payload(sequences: Sequence[str]) -> bytes:
    """The text carried by the last OSC 52 sequence, which is the copy the client keeps."""
    if not sequences:
        return b""
    match = OSC52_PATTERN.fullmatch(sequences[-1].encode("ascii", "backslashreplace"))
    if match is None:
        return b""
    return base64.b64decode(match.group(2))


def safe_name(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "-", value).strip("-")


def annotation(identifier: str, selected: str, comment: str, captured: str, created: str) -> dict[str, object]:
    return {
        "selectedText": selected,
        "capturedAt": captured,
        "context": {
            "workspace_id": "workspace-1",
            "workspace_label": "api 한",
            "tab_id": "tab-1",
            "tab_label": "server",
            "focused_pane_id": "pane-1",
            "focused_pane_cwd": "/workspace",
            "focused_pane_agent": "codex",
        },
        "id": identifier,
        "comment": comment,
        "createdAt": created,
    }


ANNOTATIONS = [
    annotation(
        "ann-one",
        "first selection with ``` ticks\nand a second line",
        "first comment\nwith two lines",
        "2026-08-08T00:00:00.000Z",
        "2026-08-08T00:00:01.000Z",
    ),
    annotation(
        "ann-two",
        "wide 한글 selection and enough text to exercise clipping at the detail edge",
        "second comment",
        "2026-08-09T10:11:12.000Z",
        "2026-08-09T10:11:13.000Z",
    ),
    annotation(
        "ann-three",
        "third selection",
        "third comment",
        "2026-08-10T20:21:22.000Z",
        "2026-08-10T20:21:23.000Z",
    ),
]
ARCHIVES = [
    {
        "version": 1,
        "id": "archive-old",
        "archivedAt": "2026-08-20T01:02:03.000Z",
        "annotations": [ANNOTATIONS[0]],
    },
    {
        "version": 1,
        "id": "archive-new",
        "archivedAt": "2026-08-21T01:02:03.000Z",
        "annotations": [ANNOTATIONS[1], ANNOTATIONS[2]],
    },
]


def write_jsonl(path: Path, records: Sequence[object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    data = b"".join(
        json.dumps(record, ensure_ascii=False, separators=(",", ":")).encode("utf-8") + b"\n"
        for record in records
    )
    path.write_bytes(data)
    path.chmod(0o600)


def seed_stores(state: Path, *, annotations: Sequence[object] = ANNOTATIONS, archives: Sequence[object] = ARCHIVES) -> None:
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    write_jsonl(state / "annotations.jsonl", annotations)
    write_jsonl(state / "archives.jsonl", archives)


def normalize_text(value: str, roots: Iterable[Path]) -> str:
    normalized = value
    for root in sorted((str(path) for path in roots), key=len, reverse=True):
        normalized = normalized.replace(root, "<ROOT>")
    normalized = PENDING_PATTERN.sub("pending-<TIME>-<PID>.json", normalized)
    normalized = TEMP_PATTERN.sub(r".\1-<PID>-<TIME>.tmp", normalized)
    normalized = HANDOFF_PATTERN.sub("herdr-annotate-<UID>", normalized)
    normalized = UUID_PATTERN.sub("<UUID>", normalized)

    def timestamp(match: re.Match[str]) -> str:
        return match.group(0) if match.group(0) in KNOWN_TIMESTAMPS else "<TIMESTAMP>"

    return ISO_PATTERN.sub(timestamp, normalized)


def normalize_bytes(value: bytes, roots: Iterable[Path]) -> bytes:
    return normalize_text(value.decode("utf-8", "replace"), roots).encode("utf-8")


def read_process_log(path: Path, roots: Iterable[Path]) -> bytes:
    if not path.exists():
        return b""
    entries = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]
    collapsed: list[dict[str, object]] = []
    for entry in entries:
        # A clipboard operation is one entry however long this platform's candidate chain is.
        role = entry.get("command")
        if (
            collapsed
            and isinstance(role, str)
            and role.startswith("clipboard-")
            and collapsed[-1].get("command") == role
        ):
            continue
        collapsed.append(entry)
    raw = "\n".join(
        json.dumps(entry, ensure_ascii=False, separators=(",", ":"), sort_keys=True)
        for entry in collapsed
    )
    return normalize_bytes((raw + ("\n" if raw else "")).encode("utf-8"), roots)


def adapter_log(process_log: Path) -> Path:
    """Where the fake clipboard tools record the adapter chain each operation walked."""
    return process_log.with_suffix(".adapters.jsonl")


def state_snapshot(path: Path, roots: Iterable[Path]) -> bytes:
    if not path.exists():
        return b"<missing>\n"
    entries: list[dict[str, object]] = []
    paths = [path, *sorted(path.rglob("*"), key=lambda item: item.relative_to(path).as_posix())]
    for item in paths:
        relative = "." if item == path else item.relative_to(path).as_posix()
        metadata = item.lstat()
        entry: dict[str, object] = {
            "path": normalize_text(relative, roots),
            "mode": f"{stat.S_IMODE(metadata.st_mode):04o}",
            "kind": "dir" if item.is_dir() else "file",
        }
        if item.is_file():
            entry["bytes"] = normalize_bytes(item.read_bytes(), roots).decode("utf-8", "replace")
        entries.append(entry)
    return (json.dumps(entries, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode("utf-8")


def create_fakes(directory: Path) -> Path:
    directory.mkdir(parents=True)
    fake = directory / "fake-process.py"
    fake.write_text(
        """#!/usr/bin/env python3
import json, os, pathlib, sys
name = pathlib.Path(sys.argv[0]).name


def append(variable, record):
    path = os.environ.get(variable)
    if not path:
        return
    target = pathlib.Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open("a", encoding="utf-8") as output:
        output.write(json.dumps(record, ensure_ascii=False, separators=(",", ":")) + "\\n")


if name == "herdr-fake":
    append("LITE_PROCESS_LOG", {"command": name, "args": sys.argv[1:]})
    if os.environ.get("LITE_HERDR_FAIL") == "1" and sys.argv[1:2] == ["plugin"]:
        sys.stderr.write(os.environ.get("LITE_HERDR_STDERR", "fake herdr failure") + "\\n")
        raise SystemExit(7)
    raise SystemExit(0)
reads = {"pbpaste", "wl-paste"}
mode = "read" if name in reads or (name == "xclip" and "-out" in sys.argv) or (name == "xsel" and "--output" in sys.argv) else "write"
# The process log keeps the role only, so it is identical on every platform; the adapter log keeps
# the platform's real program and arguments for the chain check.
append("LITE_PROCESS_LOG", {"command": "clipboard-" + mode, "args": []})
append("LITE_ADAPTER_LOG", {"role": mode, "adapter": name, "args": sys.argv[1:]})
data = sys.stdin.buffer.read() if mode == "write" else b""
if mode == "write" and os.environ.get("LITE_CLIPBOARD_OUTPUT"):
    pathlib.Path(os.environ["LITE_CLIPBOARD_OUTPUT"]).write_bytes(data)
if os.environ.get("LITE_CLIPBOARD_FAIL") in (mode, "all"):
    raise SystemExit(9)
if mode == "read":
    source = os.environ.get("LITE_CLIPBOARD_INPUT")
    if source:
        sys.stdout.buffer.write(pathlib.Path(source).read_bytes())
raise SystemExit(0)
""",
        encoding="utf-8",
    )
    fake.chmod(0o755)
    for name in ("herdr-fake", "pbpaste", "pbcopy", "wl-paste", "wl-copy", "xclip", "xsel"):
        (directory / name).symlink_to(fake.name)
    return directory


class Harness:
    def __init__(self, root: Path, binary: Path, workspace: Path, goldens: Goldens) -> None:
        self.root = root
        self.binary = binary
        self.workspace = workspace
        self.goldens = goldens
        self.fake_bin = create_fakes(workspace / "fake-bin")
        self.chains: dict[str, set[tuple[tuple[str, tuple[str, ...]], ...]]] = {
            "read": set(),
            "write": set(),
        }

    def command(self, entrypoint: str) -> list[str]:
        return [str(self.binary), entrypoint]

    def environment(
        self,
        state: Path,
        runtime: Path,
        process_log: Path,
        clipboard_input: Path,
        clipboard_output: Path,
        extra: Mapping[str, str] | None = None,
    ) -> dict[str, str]:
        env = dict(os.environ)
        env.update(
            {
                "PATH": f"{self.fake_bin}{os.pathsep}{env.get('PATH', '')}",
                "HERDR_PLUGIN_STATE_DIR": str(state),
                "HERDR_PLUGIN_ROOT": str(self.root),
                "HERDR_BIN_PATH": str(self.fake_bin / "herdr-fake"),
                "HERDR_PLUGIN_CONTEXT_JSON": "{}",
                "XDG_RUNTIME_DIR": str(runtime),
                "LITE_PROCESS_LOG": str(process_log),
                "LITE_ADAPTER_LOG": str(adapter_log(process_log)),
                "LITE_CLIPBOARD_INPUT": str(clipboard_input),
                "LITE_CLIPBOARD_OUTPUT": str(clipboard_output),
                "TZ": "UTC",
                "LANG": "en_US.UTF-8",
                "LC_ALL": "en_US.UTF-8",
                "TERM": "xterm-256color",
            }
        )
        for key in ("LITE_CLIPBOARD_FAIL", "LITE_HERDR_FAIL", "LITE_HERDR_STDERR", "HERDR_ANNOTATE_PENDING"):
            env.pop(key, None)
        if extra:
            env.update(extra)
        return env

    def run(self, entrypoint: str, env: Mapping[str, str]) -> CommandResult:
        result = subprocess.run(
            self.command(entrypoint),
            cwd=self.root,
            env=dict(env),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        return CommandResult(result.returncode, result.stdout, result.stderr)

    def record_chains(self, log: Path) -> None:
        """Keep every adapter chain a case walked, for the platform chain check."""
        if not log.exists():
            return
        entries = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines() if line]
        chain: list[tuple[str, tuple[str, ...]]] = []
        role = ""
        for entry in entries:
            if chain and entry["role"] != role:
                self.chains[role].add(tuple(chain))
                chain = []
            role = entry["role"]
            chain.append((entry["adapter"], tuple(entry["args"])))
        if chain:
            self.chains[role].add(tuple(chain))

    def case_paths(self, name: str) -> tuple[Path, Path, Path, Path, Path]:
        base = self.workspace / name
        base.mkdir(parents=True)
        runtime = base / "runtime"
        runtime.mkdir()
        clipboard_input = base / "clipboard-input"
        clipboard_input.write_bytes(b"")
        return (
            base / "state",
            runtime,
            base / "process.jsonl",
            clipboard_input,
            base / "clipboard-output",
        )

    def process_case(
        self,
        name: str,
        entrypoint: str,
        setup: Callable[[Path, Path, Path, Path, Path], Mapping[str, str] | None],
        inspect: Callable[[Path, Path, Path, Path, Path], object] | None = None,
    ) -> Path:
        state, runtime, log, clipboard_input, clipboard_output = self.case_paths(name)
        extra = setup(state, runtime, log, clipboard_input, clipboard_output) or {}
        env = self.environment(state, runtime, log, clipboard_input, clipboard_output, extra)
        result = self.run(entrypoint, env)
        self.record_chains(adapter_log(log))
        roots = [state, runtime, self.workspace, self.root]
        self.goldens.check(name, f"{name}.exit", result.exit_code)
        self.goldens.check(name, f"{name}.stdout", normalize_bytes(result.stdout, roots))
        self.goldens.check(name, f"{name}.stderr", normalize_bytes(result.stderr, roots))
        self.goldens.check(name, f"{name}.processes", read_process_log(log, roots))
        if inspect:
            self.goldens.check(
                name,
                f"{name}.artifact",
                inspect(state, runtime, log, clipboard_input, clipboard_output),
            )
        return state

    def pty_case(
        self,
        name: str,
        entrypoint: str,
        steps: Sequence[Step],
        marker: str,
        rows: int,
        cols: int,
        seed: Callable[[Path], None],
        extra: Mapping[str, str] | Callable[[Path], Mapping[str, str]] | None = None,
        compare_state: bool = False,
        compare_clipboard: bool = False,
    ) -> Path:
        state, runtime, log, clipboard_input, clipboard_output = self.case_paths(name)
        clipboard_input.write_bytes(b"clipboard selection")
        seed(state)
        case_extra = extra(state) if callable(extra) else extra
        env = self.environment(state, runtime, log, clipboard_input, clipboard_output, case_extra)
        session = PtySession(self.command(entrypoint), env, self.root, rows, cols)
        session.wait_for(marker)
        screens = [("initial", session.grid.snapshot())]
        for step in steps:
            self.goldens.coverage.update(step.coverage)
            if step.process_signal is None:
                session.send(step.data)
                if step.exits:
                    session.wait_exit()
            else:
                session.send_signal(step.process_signal)
            screens.append((step.label, session.grid.snapshot()))
        exit_code = session.finish()
        osc52 = osc52_sequences(bytes(session.raw))
        self.record_chains(adapter_log(log))
        roots = [state, runtime, self.workspace, self.root]
        self.goldens.check(name, f"{name}.exit", exit_code)
        self.goldens.check(name, f"{name}.screen-count", len(screens))
        for label, screen in screens:
            self.goldens.check(name, f"{name}.screen.{label}", screen, screen=True)
        self.goldens.check(name, f"{name}.processes", read_process_log(log, roots))
        self.goldens.check(name, f"{name}.osc52", osc52)
        if compare_clipboard:
            clipboard = normalize_bytes(
                clipboard_output.read_bytes() if clipboard_output.exists() else b"", roots
            )
            self.goldens.check(name, f"{name}.clipboard", clipboard)
            # Every pane copy reaches the viewing client too: the payload the terminal received is
            # byte-for-byte the text the native writer was handed.
            self.goldens.equal(
                f"{name}.osc52-payload",
                clipboard,
                normalize_bytes(osc52_payload(osc52), roots),
            )
        if compare_state:
            self.goldens.check(name, f"{name}.state", state_snapshot(state, roots))
        return state


def pending_artifact(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> object:
    del runtime, log, source, sink
    files = sorted(state.glob("pending-*.json"))
    if len(files) != 1:
        return {"pending_count": len(files)}
    file = files[0]
    return {
        "name": PENDING_PATTERN.sub("pending-<TIME>-<PID>.json", file.name),
        "mode": f"{stat.S_IMODE(file.stat().st_mode):04o}",
        "bytes": normalize_bytes(file.read_bytes(), [state]).decode("utf-8"),
    }


def pending_and_runtime_artifact(state: Path,
    runtime: Path,
    log: Path,
    source: Path,
    sink: Path,
) -> object:
    return {
        "pending": pending_artifact(state, runtime, log, source, sink),
        "runtime": state_snapshot(runtime, [runtime]).decode("utf-8"),
    }


def clipboard_artifact(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> object:
    del state, runtime, log, source
    return sink.read_bytes() if sink.exists() else b""


def clipboard_and_state_artifact(state: Path, runtime: Path, log: Path, source: Path, sink: Path
) -> object:
    del runtime, log, source
    return {
        "clipboard": (sink.read_bytes() if sink.exists() else b"").decode("utf-8", "replace"),
        "state": state_snapshot(state, [state]).decode("utf-8", "replace"),
    }


def no_pending_artifact(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> object:
    del runtime, log, source, sink
    return sorted(path.name for path in state.glob("pending-*.json")) if state.exists() else []


def run_process_layer(harness: Harness) -> None:
    context = json.dumps(
        {
            "selected_text": "context selection 한글",
            "workspace_id": "workspace-1",
            "workspace_label": "api 한",
            "tab_id": "tab-1",
            "tab_label": "server",
            "focused_pane_id": "pane-1",
            "focused_pane_cwd": "/workspace",
            "focused_pane_agent": "codex",
            "ignored": "not persisted",
        },
        ensure_ascii=False,
        separators=(",", ":"),
    )

    def handoff_path(runtime: Path) -> Path:
        return runtime / f"herdr-annotate-{os.getuid() if hasattr(os, 'getuid') else 'user'}" / "selection"

    def capture_context(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> Mapping[str, str]:
        del log, sink
        state.mkdir()
        handoff = handoff_path(runtime)
        handoff.parent.mkdir()
        handoff.write_text("lower-priority handoff", encoding="utf-8")
        source.write_text("lower-priority clipboard", encoding="utf-8")
        return {"HERDR_PLUGIN_CONTEXT_JSON": context}

    harness.process_case(
        "process.capture.context", "capture", capture_context, pending_and_runtime_artifact
    )

    def capture_handoff(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del log, sink
        state.mkdir()
        handoff = handoff_path(runtime)
        handoff.parent.mkdir()
        handoff.write_text("handoff selection\n", encoding="utf-8")
        source.write_text("lower-priority clipboard", encoding="utf-8")

    harness.process_case(
        "process.capture.handoff", "capture", capture_handoff, pending_and_runtime_artifact
    )

    def capture_stale_handoff(state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> None:
        del log, sink
        state.mkdir()
        handoff = handoff_path(runtime)
        handoff.parent.mkdir()
        handoff.write_text("stale handoff", encoding="utf-8")
        stale = time.time() - 16
        os.utime(handoff, (stale, stale))
        source.write_text("clipboard after stale handoff", encoding="utf-8")

    harness.process_case(
        "process.capture.stale-handoff",
        "capture",
        capture_stale_handoff,
        pending_and_runtime_artifact,
    )

    def capture_blank_handoff(state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> None:
        del log, sink
        state.mkdir()
        handoff = handoff_path(runtime)
        handoff.parent.mkdir()
        handoff.write_text(" \n\t", encoding="utf-8")
        source.write_text("clipboard after blank handoff", encoding="utf-8")

    harness.process_case(
        "process.capture.blank-handoff",
        "capture",
        capture_blank_handoff,
        pending_and_runtime_artifact,
    )

    def capture_invalid_utf8_handoff(state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> None:
        del log, sink
        state.mkdir()
        handoff = handoff_path(runtime)
        handoff.parent.mkdir()
        handoff.write_bytes(b"invalid-\xff-handoff")
        source.write_text("clipboard after invalid handoff", encoding="utf-8")

    harness.process_case(
        "process.capture.invalid-utf8-handoff",
        "capture",
        capture_invalid_utf8_handoff,
        pending_and_runtime_artifact,
    )

    def capture_clipboard(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, sink
        state.mkdir()
        source.write_bytes("clipboard 한 selection".encode("utf-8"))

    harness.process_case("process.capture.clipboard", "capture", capture_clipboard, pending_artifact)

    def capture_invalid_context(state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> Mapping[str, str]:
        del runtime, log, sink
        state.mkdir()
        source.write_text("clipboard after invalid context", encoding="utf-8")
        return {"HERDR_PLUGIN_CONTEXT_JSON": "{broken"}

    harness.process_case(
        "process.capture.invalid-context", "capture", capture_invalid_context, pending_artifact
    )

    def capture_empty(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, sink
        state.mkdir()
        source.write_bytes(b" \n\t")

    harness.process_case("process.capture.empty", "capture", capture_empty, no_pending_artifact)

    def capture_no_clipboard(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> Mapping[str, str]:
        del runtime, log, source, sink
        state.mkdir()
        return {"LITE_CLIPBOARD_FAIL": "read"}

    harness.process_case("process.capture.no-clipboard", "capture", capture_no_clipboard, no_pending_artifact)

    def capture_open_failure(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> Mapping[str, str]:
        del runtime, log, source, sink
        state.mkdir()
        return {
            "HERDR_PLUGIN_CONTEXT_JSON": context,
            "LITE_HERDR_FAIL": "1",
            "LITE_HERDR_STDERR": "pane open failed",
        }

    harness.process_case("process.capture.open-failure", "capture", capture_open_failure, no_pending_artifact)

    def capture_missing_state(state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> Mapping[str, str]:
        del state, runtime, log, source, sink
        return {"HERDR_PLUGIN_STATE_DIR": "", "HERDR_PLUGIN_CONTEXT_JSON": context}

    harness.process_case("process.capture.missing-state", "capture", capture_missing_state)

    def capture_missing_root(state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> Mapping[str, str]:
        del runtime, log, source, sink
        state.mkdir()
        return {"HERDR_PLUGIN_ROOT": "", "HERDR_PLUGIN_CONTEXT_JSON": context}

    harness.process_case("process.capture.missing-root", "capture", capture_missing_root)

    def copy_empty(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del state, runtime, log, source, sink

    harness.process_case(
        "process.copy.empty",
        "copy-context",
        copy_empty,
        lambda state, runtime, log, source, sink: state_snapshot(state, [state]),
    )

    def copy_populated(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, source, sink
        seed_stores(state, archives=[])

    harness.process_case("process.copy.populated", "copy-context", copy_populated, clipboard_artifact)

    def copy_single(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, source, sink
        seed_stores(state, annotations=ANNOTATIONS[:1], archives=[])

    harness.process_case("process.copy.single", "copy-context", copy_single, clipboard_artifact)

    def copy_no_clipboard(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> Mapping[str, str]:
        del runtime, log, source, sink
        seed_stores(state, archives=[])
        return {"LITE_CLIPBOARD_FAIL": "write"}

    harness.process_case("process.copy.no-clipboard", "copy-context", copy_no_clipboard, clipboard_artifact)

    def copy_invalid(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, source, sink
        state.mkdir()
        (state / "annotations.jsonl").write_text("{broken\n", encoding="utf-8")

    harness.process_case("process.copy.invalid-store", "copy-context", copy_invalid)

    def copy_missing_state(state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> Mapping[str, str]:
        del state, runtime, log, source, sink
        return {"HERDR_PLUGIN_STATE_DIR": ""}

    harness.process_case("process.copy.missing-state", "copy-context", copy_missing_state)

    def fresh_lock(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, source, sink
        state.mkdir()
        (state / ".annotations.lock").mkdir()

    harness.process_case(
        "process.copy.busy-lock",
        "copy-context",
        fresh_lock,
        lambda state, runtime, log, source, sink: state_snapshot(state, [state]),
    )

    def stale_lock(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        fresh_lock(state, runtime, log, source, sink)
        stale = time.time() - 31
        os.utime(state / ".annotations.lock", (stale, stale))

    harness.process_case(
        "process.copy.stale-lock",
        "copy-context",
        stale_lock,
        lambda state, runtime, log, source, sink: state_snapshot(state, [state]),
    )

    def copy_archive_empty(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del state, runtime, log, source, sink

    harness.process_case(
        "process.copy-archive.empty",
        "copy-archive",
        copy_archive_empty,
        clipboard_and_state_artifact,
    )

    def copy_archive_populated(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, source, sink
        seed_stores(state)

    harness.process_case(
        "process.copy-archive.populated",
        "copy-archive",
        copy_archive_populated,
        clipboard_and_state_artifact,
    )

    def copy_archive_no_clipboard(state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> Mapping[str, str]:
        del runtime, log, source, sink
        seed_stores(state)
        return {"LITE_CLIPBOARD_FAIL": "write"}

    harness.process_case(
        "process.copy-archive.no-clipboard",
        "copy-archive",
        copy_archive_no_clipboard,
        clipboard_and_state_artifact,
    )

    def copy_archive_missing_state(state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> Mapping[str, str]:
        del state, runtime, log, source, sink
        return {"HERDR_PLUGIN_STATE_DIR": ""}

    harness.process_case("process.copy-archive.missing-state", "copy-archive", copy_archive_missing_state)

    def manage_success(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del state, runtime, log, source, sink

    harness.process_case("process.manage.success", "manage", manage_success)

    def manage_failure(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> Mapping[str, str]:
        del state, runtime, log, source, sink
        return {"LITE_HERDR_FAIL": "1", "LITE_HERDR_STDERR": "manager open failed"}

    harness.process_case("process.manage.failure", "manage", manage_failure)

    def manage_failure_without_stderr(state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> Mapping[str, str]:
        del state, runtime, log, source, sink
        return {"LITE_HERDR_FAIL": "1", "LITE_HERDR_STDERR": ""}

    harness.process_case(
        "process.manage.failure-without-stderr", "manage", manage_failure_without_stderr
    )

    def manage_missing_root(state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> Mapping[str, str]:
        del state, runtime, log, source, sink
        return {"HERDR_PLUGIN_ROOT": ""}

    harness.process_case("process.manage.missing-root", "manage", manage_missing_root)

    def editor_missing(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, source, sink
        state.mkdir()

    harness.process_case("process.editor.missing-pending", "editor", editor_missing)

    def editor_invalid(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> Mapping[str, str]:
        del runtime, log, source, sink
        state.mkdir()
        pending = state / "invalid-pending.json"
        pending.write_text('{"selectedText":"only"}\n', encoding="utf-8")
        return {"HERDR_ANNOTATE_PENDING": str(pending)}

    harness.process_case("process.editor.invalid-pending", "editor", editor_invalid)

    def manager_missing(state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> Mapping[str, str]:
        del state, runtime, log, source, sink
        return {"HERDR_PLUGIN_STATE_DIR": ""}

    harness.process_case("process.manager.missing-state", "manager", manager_missing)


EDITOR_REQUIRED = {
    "editor:chars",
    "editor:enter",
    "editor:backspace",
    "editor:delete",
    "editor:left",
    "editor:right",
    "editor:up",
    "editor:down",
    "editor:home",
    "editor:end",
    "editor:ctrl-s",
    "editor:esc",
    "editor:ctrl-c",
    "editor:alt-left",
    "editor:alt-right",
    "editor:ctrl-left",
    "editor:ctrl-right",
    "editor:super-left",
    "editor:super-right",
    "editor:alt-b",
    "editor:alt-f",
    "editor:alt-backspace",
    "editor:ctrl-w",
    "editor:ctrl-u",
    "editor:ctrl-a",
    "editor:ctrl-e",
}
MANAGER_REQUIRED = {
    f"manager:{view}:{key}"
    for view in ("active", "archives")
    for key in ("j", "k", "y", "c", "C", "d", "D", "r", "u", "Tab", "Esc", "q", "Ctrl-C")
}


def editor_seed(state: Path) -> None:
    state.mkdir(parents=True, mode=0o700)


def manager_seed(state: Path) -> None:
    seed_stores(state)


def empty_manager_seed(state: Path) -> None:
    state.mkdir(parents=True, mode=0o700)


def pending_file_extra(state: Path) -> Mapping[str, str]:
    pending = state / "pending-input.json"
    pending.write_text(
        json.dumps(
            {
                "selectedText": "selection from pending file",
                "context": {"workspace_label": "pending workspace", "tab_label": "pending tab"},
                "capturedAt": "2026-08-08T00:00:00.000Z",
            },
            separators=(",", ":"),
        )
        + "\n",
        encoding="utf-8",
    )
    return {"HERDR_ANNOTATE_PENDING": str(pending)}


def run_screen_and_store_layer(harness: Harness) -> Path:
    editor_context = {
        "HERDR_PLUGIN_CONTEXT_JSON": json.dumps(
            {
                "selected_text": "selected wide 한글 text\nwith a second line",
                "workspace_label": "api 한",
                "tab_label": "server",
            },
            ensure_ascii=False,
            separators=(",", ":"),
        )
    }
    editor_steps = [
        Step("chars", "alpha 한글 e\u0301".encode(), ("editor:chars",)),
        Step("enter", b"\r", ("editor:enter",)),
        Step("chars-second-line", b"beta", ("editor:chars",)),
        Step("home", b"\x1b[H", ("editor:home",)),
        Step("right", b"\x1b[C", ("editor:right",)),
        Step("delete", b"\x1b[3~", ("editor:delete",)),
        Step("end", b"\x1b[F", ("editor:end",)),
        Step("backspace", b"\x7f", ("editor:backspace",)),
        Step("up", b"\x1b[A", ("editor:up",)),
        Step("left", b"\x1b[D", ("editor:left",)),
        Step("down", b"\x1b[B", ("editor:down",)),
        Step("save", b"\x13", ("editor:ctrl-s",)),
    ]
    editor_state = harness.pty_case(
        "screen.editor.edit-save",
        "editor",
        editor_steps,
        "Selected text",
        22,
        86,
        editor_seed,
        editor_context,
        compare_state=True,
    )
    # Word and line editing keys. A compared screen holds cells, not the cursor, so every move is
    # followed by a marker character: the saved text is the record of where both runtimes put the
    # cursor. Alt/Option is xterm modifier 3, Ctrl is 5, and Command/Super is 9.
    move_steps = [
        Step("chars", b"alpha beta gamma", ("editor:chars",)),
        Step("enter", b"\r", ("editor:enter",)),
        Step("chars-second-line", "\ud55c\uae00 delta epsilon".encode(), ("editor:chars",)),
        Step("alt-left", b"\x1b[1;3D", ("editor:alt-left",)),
        Step("mark-1", b"1"),
        Step("alt-b", b"\x1bb", ("editor:alt-b",)),
        Step("mark-2", b"2"),
        Step("alt-f", b"\x1bf", ("editor:alt-f",)),
        Step("mark-3", b"3"),
        Step("alt-right", b"\x1b[1;3C", ("editor:alt-right",)),
        Step("mark-4", b"4"),
        Step("ctrl-left", b"\x1b[1;5D", ("editor:ctrl-left",)),
        Step("mark-5", b"5"),
        Step("ctrl-right", b"\x1b[1;5C", ("editor:ctrl-right",)),
        Step("mark-6", b"6"),
        # A word move crosses a newline in a step of its own: three presses walk back over the words
        # of the second line, the fourth lands on the end of the first, and coming back needs one
        # press for the newline and one for the next word.
        Step("alt-left-word", b"\x1b[1;3D", ("editor:alt-left",)),
        Step("alt-left-previous-word", b"\x1b[1;3D", ("editor:alt-left",)),
        Step("alt-left-line-first-word", b"\x1b[1;3D", ("editor:alt-left",)),
        Step("alt-left-over-newline", b"\x1b[1;3D", ("editor:alt-left",)),
        Step("mark-7", b"7"),
        Step("alt-right-over-newline", b"\x1b[1;3C", ("editor:alt-right",)),
        Step("alt-right-next-word", b"\x1b[1;3C", ("editor:alt-right",)),
        Step("mark-8", b"8"),
        Step("super-left", b"\x1b[1;9D", ("editor:super-left",)),
        Step("mark-9", b"9"),
        Step("super-right", b"\x1b[1;9C", ("editor:super-right",)),
        Step("mark-a", b"A"),
        Step("ctrl-a", b"\x01", ("editor:ctrl-a",)),
        Step("mark-b", b"B"),
        Step("ctrl-e", b"\x05", ("editor:ctrl-e",)),
        Step("mark-c", b"C"),
        Step("save", b"\x13", ("editor:ctrl-s",)),
    ]
    harness.pty_case(
        "screen.editor.word-line-moves",
        "editor",
        move_steps,
        "Selected text",
        22,
        86,
        editor_seed,
        editor_context,
        compare_state=True,
    )
    kill_steps = [
        Step("chars", b"alpha beta gamma", ("editor:chars",)),
        Step("enter", b"\r", ("editor:enter",)),
        Step("chars-second-line", "\ud55c\uae00 delta epsilon".encode(), ("editor:chars",)),
        Step("ctrl-w", b"\x17", ("editor:ctrl-w",)),
        Step("mark-w", b"W"),
        Step("alt-backspace-marker", b"\x1b\x7f", ("editor:alt-backspace",)),
        Step("alt-backspace-word", b"\x1b\x7f", ("editor:alt-backspace",)),
        Step("mark-x", b"X"),
        Step("ctrl-u", b"\x15", ("editor:ctrl-u",)),
        Step("mark-y", b"Y"),
        # At a line start the kill takes the newline itself, one step, and joins the two lines.
        Step("super-left-to-line-start", b"\x1b[1;9D", ("editor:super-left",)),
        Step("alt-backspace-newline", b"\x1b\x7f", ("editor:alt-backspace",)),
        Step("mark-z", b"Z"),
        Step("ctrl-a-line-start", b"\x01", ("editor:ctrl-a",)),
        # Killing back to a line start the cursor already sits on removes nothing.
        Step("ctrl-u-at-line-start", b"\x15", ("editor:ctrl-u",)),
        Step("mark-tail", b"tail ", ("editor:chars",)),
        Step("save", b"\x13", ("editor:ctrl-s",)),
    ]
    harness.pty_case(
        "screen.editor.word-line-kills",
        "editor",
        kill_steps,
        "Selected text",
        22,
        86,
        editor_seed,
        editor_context,
        compare_state=True,
    )
    harness.pty_case(
        "screen.editor.pending-file-save",
        "editor",
        [Step("chars", b"pending comment"), Step("save", b"\x13")],
        "Selected text",
        22,
        86,
        editor_seed,
        pending_file_extra,
        compare_state=True,
    )
    harness.pty_case(
        "screen.editor.empty-save-escape",
        "editor",
        [
            Step("empty-save", b"\x13", ("editor:ctrl-s",)),
            Step("escape", b"\x1b", ("editor:esc",)),
        ],
        "Selected text",
        22,
        86,
        editor_seed,
        editor_context,
    )
    harness.pty_case(
        "screen.editor.control-c",
        "editor",
        [Step("control-c", b"\x03", ("editor:ctrl-c",))],
        "Selected text",
        22,
        86,
        editor_seed,
        editor_context,
    )
    harness.pty_case(
        "screen.editor.missing-state",
        "editor",
        [Step("char", b"x"), Step("save", b"\x13"), Step("escape", b"\x1b")],
        "Selected text",
        22,
        86,
        editor_seed,
        {**editor_context, "HERDR_PLUGIN_STATE_DIR": ""},
    )
    harness.pty_case(
        "screen.editor.sigterm",
        "editor",
        [Step("sigterm", process_signal=signal.SIGTERM)],
        "Selected text",
        22,
        86,
        editor_seed,
        editor_context,
    )

    manager_steps = [
        Step("active-j", b"j", ("manager:active:j",)),
        Step("active-k", b"k", ("manager:active:k",)),
        Step("active-arrow-down", b"\x1b[B"),
        Step("active-arrow-up", b"\x1b[A"),
        Step("active-delete", b"d", ("manager:active:d",)),
        Step("active-clear-confirm", b"D", ("manager:active:D",)),
        Step("active-clear-cancel", b"\x1b", ("manager:active:Esc",)),
        Step("active-clear-confirm-again", b"D", ("manager:active:D",)),
        Step("active-clear", b"D", ("manager:active:D",)),
        Step("active-reload", b"r", ("manager:active:r",)),
        Step("active-u-ignored", b"u", ("manager:active:u",)),
        Step("active-to-archives", b"\t", ("manager:active:Tab",)),
        Step("archives-j", b"j", ("manager:archives:j",)),
        Step("archives-k", b"k", ("manager:archives:k",)),
        Step("archives-arrow-down", b"\x1b[B"),
        Step("archives-arrow-up", b"\x1b[A"),
        Step("archives-reload", b"r", ("manager:archives:r",)),
        Step("archives-c-ignored", b"c", ("manager:archives:c",)),
        Step("archives-C-ignored", b"C", ("manager:archives:C",)),
        Step("archives-D-ignored", b"D", ("manager:archives:D",)),
        Step("archives-delete-confirm", b"d", ("manager:archives:d",)),
        Step("archives-delete-cancel", b"\x1b", ("manager:archives:Esc",)),
        Step("archives-restore", b"u", ("manager:archives:u",)),
        Step("archives-delete-confirm-again", b"d", ("manager:archives:d",)),
        Step("archives-delete", b"d", ("manager:archives:d",)),
        Step("archives-to-active", b"\t", ("manager:archives:Tab",)),
        Step("active-quit", b"q", ("manager:active:q",)),
    ]
    harness.pty_case(
        "screen.manager.all-views",
        "manager",
        manager_steps,
        "Annotations (",
        28,
        98,
        manager_seed,
        compare_state=True,
        compare_clipboard=True,
    )
    harness.pty_case(
        "screen.manager.escape-exit",
        "manager",
        [
            Step("to-archives", b"\t", ("manager:active:Tab",)),
            Step("archives-escape", b"\x1b", ("manager:archives:Esc",)),
        ],
        "Annotations (",
        28,
        98,
        manager_seed,
    )
    harness.pty_case(
        "screen.manager.escape-active",
        "manager",
        [Step("active-escape", b"\x1b", ("manager:active:Esc",))],
        "Annotations (",
        28,
        98,
        manager_seed,
    )
    harness.pty_case(
        "screen.manager.control-c-active",
        "manager",
        [Step("control-c", b"\x03", ("manager:active:Ctrl-C",))],
        "Annotations (",
        28,
        98,
        manager_seed,
    )
    harness.pty_case(
        "screen.manager.control-c-archives",
        "manager",
        [
            Step("to-archives", b"\t", ("manager:active:Tab",)),
            Step("control-c", b"\x03", ("manager:archives:Ctrl-C",)),
        ],
        "Annotations (",
        28,
        98,
        manager_seed,
    )
    harness.pty_case(
        "screen.manager.q-archives",
        "manager",
        [
            Step("to-archives", b"\t", ("manager:active:Tab",)),
            Step("archives-q", b"q", ("manager:archives:q",)),
        ],
        "Annotations (",
        28,
        98,
        manager_seed,
    )
    harness.pty_case(
        "screen.manager.sighup",
        "manager",
        [Step("sighup", process_signal=signal.SIGHUP)],
        "Annotations (",
        28,
        98,
        manager_seed,
    )
    harness.pty_case(
        "screen.manager.empty-actions",
        "manager",
        [
            Step("active-y", b"y"),
            Step("active-c", b"c"),
            Step("active-copy-archive", b"C"),
            Step("to-archives", b"\t"),
            Step("archives-y", b"y"),
            Step("archives-u", b"u"),
            Step("archives-d", b"d"),
            Step("quit", b"q"),
        ],
        "Annotations (",
        28,
        98,
        empty_manager_seed,
        compare_state=True,
        compare_clipboard=True,
    )

    for key, steps in (
        ("active-y-success", [Step("copy-one", b"y", ("manager:active:y",), exits=True)]),
        ("active-c-success", [Step("copy-all", b"c", ("manager:active:c",), exits=True)]),
        (
            "archives-y-success",
            [
                Step("to-archives", b"\t", ("manager:active:Tab",)),
                Step("copy-archive", b"y", ("manager:archives:y",), exits=True),
            ],
        ),
    ):
        harness.pty_case(
            f"store.manager.{key}",
            "manager",
            steps,
            "Annotations (",
            28,
            98,
            manager_seed,
            compare_state=True,
            compare_clipboard=True,
        )

    harness.pty_case(
        "store.manager.copy-archive",
        "manager",
        [Step("copy-archive", b"C", ("manager:active:C",), exits=True)],
        "Annotations (",
        28,
        98,
        manager_seed,
        compare_state=True,
        compare_clipboard=True,
    )

    # A pane copy on a machine with no working clipboard writer, which is the remote-server shape of
    # issue #40: the native write fails, the OSC 52 sequence still reaches the viewing client, and the
    # copy is a success. `C` must therefore still archive and clear the active list.
    for key, steps in (
        ("osc52-remote-copy", [Step("copy-one", b"y", ("manager:active:y",), exits=True)]),
        ("osc52-remote-copy-all", [Step("copy-all", b"c", ("manager:active:c",), exits=True)]),
        ("osc52-remote-copy-archive", [Step("copy-archive", b"C", ("manager:active:C",), exits=True)]),
    ):
        state = harness.pty_case(
            f"store.manager.{key}",
            "manager",
            steps,
            "Annotations (",
            28,
            98,
            manager_seed,
            {"LITE_CLIPBOARD_FAIL": "write"},
            compare_state=True,
            compare_clipboard=True,
        )
        if key != "osc52-remote-copy-archive":
            continue

        # The copy succeeded on the OSC 52 path alone, so copy-and-archive must have gone on to
        # write its archive (a third, beside the two seeded) and clear the active list, rather than
        # stopping at the copy.
        def records(name: str, state: Path = state) -> int:
            path = state / name
            if not path.exists():
                return 0
            return len([line for line in path.read_text(encoding="utf-8").splitlines() if line.strip()])

        harness.goldens.equal(
            f"store.manager.{key}.archived",
            {"archives": 3, "active": 0},
            {"archives": records("archives.jsonl"), "active": records("annotations.jsonl")},
        )

    harness.goldens.require_coverage(EDITOR_REQUIRED | MANAGER_REQUIRED)
    return editor_state


def verify_cross_read(harness: Harness, editor_state: Path) -> None:
    """The runtime still reads a store the retired Bun editor wrote.

    The fixture beside the goldens is the exact `annotations.jsonl` the Bun editor produced in the
    recorded `screen.editor.edit-save` run, so this stays a compatibility check after Bun is gone.
    """
    fixture = harness.goldens.directory / "fixtures" / "bun-editor-annotations.jsonl"
    if harness.goldens.record:
        fixture.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(editor_state / "annotations.jsonl", fixture)
    state, runtime, log, clipboard_input, clipboard_output = harness.case_paths("store.cross-read")
    state.mkdir(mode=0o700)
    shutil.copyfile(fixture, state / "annotations.jsonl")
    (state / "annotations.jsonl").chmod(0o600)
    result = harness.run(
        "copy-context",
        harness.environment(state, runtime, log, clipboard_input, clipboard_output),
    )
    harness.record_chains(adapter_log(log))
    roots = [state, runtime, harness.workspace, harness.root]
    group = "store.cross-read"
    harness.goldens.check(group, f"{group}.exit", result.exit_code)
    harness.goldens.check(group, f"{group}.stderr", normalize_bytes(result.stderr, roots))
    harness.goldens.check(
        group,
        f"{group}.markdown",
        clipboard_output.read_bytes() if clipboard_output.exists() else b"",
    )
    harness.goldens.check(group, f"{group}.processes", read_process_log(log, roots))


def verify_clipboard_chains(harness: Harness) -> None:
    """The chains the run walked are this platform's candidate list, in order.

    The goldens keep the clipboard role only so they hold on every platform. The adapter identity,
    its arguments, and the fallback order are checked here instead, including that the run
    exercised both a first-adapter success and the full chain.
    """
    candidates = CLIPBOARD_CANDIDATES["darwin" if sys.platform == "darwin" else "linux"]

    def steps(chain: Sequence[tuple[str, Sequence[str]]]) -> list[list[object]]:
        return [[adapter, list(arguments)] for adapter, arguments in chain]

    for role, expected in candidates.items():
        observed = sorted(harness.chains[role], key=len)
        harness.goldens.equal(
            f"clipboard.{role}.chains",
            [steps(expected[: len(chain)]) for chain in observed],
            [steps(chain) for chain in observed],
        )
        harness.goldens.equal(
            f"clipboard.{role}.lengths",
            sorted({1, len(expected)}),
            sorted({len(chain) for chain in observed}),
        )


# Every user-visible message the retired Bun runtime produced, and the native file that owns it.
ERROR_CATALOG = {
    "HERDR_PLUGIN_STATE_DIR is not set": "rust/src/cli.rs",
    "HERDR_PLUGIN_ROOT is not set": "rust/src/cli.rs",
    "No supported clipboard reader is available": "rust/src/clipboard.rs",
    "No supported clipboard writer is available": "rust/src/clipboard.rs",
    "Missing pending annotation": "rust/src/editor.rs",
    "Pending annotation is invalid": "rust/src/editor.rs",
    "Write a comment before saving.": "rust/src/editor.rs",
    "Plugin state directory is unavailable.": "rust/src/editor.rs",
    "Nothing to copy.": "rust/src/manager_copy.rs",
    "Nothing to copy and archive.": "rust/src/archive_workflow.rs",
    "No archive selected.": "rust/src/manager.rs",
    "Unable to save annotation": "rust/src/store.rs",
    "Unable to update annotations": "rust/src/store.rs",
    "Unable to update archives": "rust/src/store.rs",
    "Unable to read": "rust/src/store.rs",
    "Unable to access": "rust/src/store.rs",
    "Unable to lock": "rust/src/store.rs",
    "are busy; try again.": "rust/src/store.rs",
    "Copied and archived, but active annotations remain:": "rust/src/manager.rs",
    "Annotations copied and archived": "rust/src/cli.rs",
    "Copy and archive failed": "rust/src/cli.rs",
    "Copy and archive incomplete": "rust/src/cli.rs",
    "copied as Markdown and archived.": "rust/src/cli.rs",
    "Annotations restored, but the archive remains:": "rust/src/manager.rs",
}


def verify_error_catalog(root: Path, goldens: Goldens) -> None:
    for message, source in ERROR_CATALOG.items():
        goldens.equal(
            f"errors.catalog.{safe_name(message)}",
            True,
            message in (root / source).read_text(encoding="utf-8"),
        )


def manifest_entries(manifest: Mapping[str, object], table: str) -> dict[str, dict[str, object]]:
    entries = manifest.get(table, [])
    if not isinstance(entries, list):
        return {}
    return {
        str(item.get("id")): item
        for item in entries
        if isinstance(item, dict) and isinstance(item.get("id"), str)
    }


LITE_UNIX_BUILD = ["bash", "../scripts/fetch-herdr-annotate.sh"]
LITE_WINDOWS_BUILD = [
    "powershell.exe",
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    "../scripts/fetch-herdr-annotate.ps1",
]


def verify_manifests(root: Path, goldens: Goldens) -> None:
    """The Lite variant and the Full plugin declare one entrypoint set, and the harness drives it.

    Lite is the same runtime reached from one directory down, so the two manifests must agree on
    every declaration a user sees, and neither may gate the annotation tools to a platform.
    """

    def load(path: Path) -> dict[str, object]:
        with path.open("rb") as handle:
            return tomllib.load(handle)

    lite = load(root / "lite" / "herdr-plugin.toml")
    full = load(root / "herdr-plugin.toml")

    for field in ("id", "name", "version", "min_herdr_version", "description", "platforms"):
        goldens.equal(f"manifest.{field}", full.get(field), lite.get(field))

    goldens.equal(
        "manifest.lite-build-commands",
        [LITE_UNIX_BUILD, LITE_WINDOWS_BUILD],
        [item.get("command") for item in lite.get("build", []) if isinstance(item, dict)],
    )

    lite_actions = manifest_entries(lite, "actions")
    lite_panes = manifest_entries(lite, "panes")
    full_actions = manifest_entries(full, "actions")
    full_panes = manifest_entries(full, "panes")

    goldens.equal(
        "manifest.harness-entrypoints",
        list(ENTRYPOINTS),
        sorted(set(lite_actions) | set(lite_panes)),
    )
    # Full adds the plannotator-tui review entrypoints; every Lite one must also be in Full.
    goldens.equal(
        "manifest.pane-ids",
        sorted(lite_panes),
        sorted(set(full_panes) & set(lite_panes)),
    )
    goldens.equal(
        "manifest.action-ids",
        sorted(lite_actions),
        sorted(set(full_actions) & set(lite_actions)),
    )

    for table, lite_entries, full_entries, fields in (
        ("action", lite_actions, full_actions, ("title", "description", "contexts")),
        ("pane", lite_panes, full_panes, ("title", "placement", "width", "height")),
    ):
        for identifier, entry in lite_entries.items():
            full_entry = full_entries.get(identifier, {})
            for field in fields:
                goldens.equal(
                    f"manifest.{table}.{identifier}.{field}",
                    entry.get(field),
                    full_entry.get(field),
                )
            goldens.equal(
                f"manifest.{table}.{identifier}.command",
                ([LITE_NATIVE_PROGRAM, identifier], [NATIVE_PROGRAM, identifier]),
                (entry.get("command"), full_entry.get("command")),
            )
            goldens.equal(
                f"manifest.{table}.{identifier}.platforms",
                (None, None),
                (entry.get("platforms"), full_entry.get("platforms")),
            )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Check Herdr Annotate Lite against its goldens.")
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument(
        "--goldens",
        type=Path,
        default=None,
        help="the recorded expectations (default: scripts/lite-goldens beside this file)",
    )
    parser.add_argument(
        "--record",
        action="store_true",
        help="rewrite the goldens from this run, for a deliberate behavior change",
    )
    arguments = parser.parse_args()
    if arguments.goldens is None:
        arguments.goldens = Path(__file__).resolve().parent / "lite-goldens"
    return arguments


def main() -> int:
    args = parse_args()
    os.umask(0o022)
    workspace = Path(tempfile.mkdtemp(prefix="herdr-annotate-lite-"))
    goldens = Goldens(args.goldens.resolve(), workspace / "artifacts", record=args.record)
    try:
        harness = Harness(args.root.resolve(), args.binary.resolve(), workspace, goldens)
        print("== process layer")
        run_process_layer(harness)
        print("== screen and store layers")
        editor_state = run_screen_and_store_layer(harness)
        print("== cross-read, clipboard chains, error catalog, and manifests")
        verify_cross_read(harness, editor_state)
        verify_clipboard_chains(harness)
        verify_error_catalog(args.root.resolve(), goldens)
        verify_manifests(args.root.resolve(), goldens)
        goldens.finish()
        summary = (
            f"Lite regression: {goldens.checked} observables, {goldens.screens} screens, "
            f"{len(DELIBERATE_DIFFERENCES)} deliberate difference"
        )
        if goldens.failures:
            print(
                f"{summary}, {len(goldens.failures)} failures; artifacts: {workspace}",
                file=sys.stderr,
            )
            return 1
        if args.record:
            print(f"{summary}, recorded into {goldens.directory}")
        else:
            print(f"{summary}, zero failures")
        shutil.rmtree(workspace)
        return 0
    except Exception as error:  # noqa: BLE001 - the harness must retain evidence on any failure.
        print(f"lite-regression: {error}; artifacts: {workspace}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
