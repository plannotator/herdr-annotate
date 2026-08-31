#!/usr/bin/env python3
"""Deterministic differential harness for Herdr Annotate Lite."""

from __future__ import annotations

import argparse
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
PENDING_PATTERN = re.compile(r"pending-\d+-\d+\.json")
TEMP_PATTERN = re.compile(r"\.(annotations|archives)-\d+-\d+\.tmp")
DELIBERATE_DIVERGENCES = ("manager timestamp locale outside en-US",)

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


@dataclass
class PtyResult:
    screens: list[tuple[str, tuple[tuple[str, ...], ...]]]
    exit_code: int


class Proof:
    def __init__(self, artifacts: Path) -> None:
        self.artifacts = artifacts
        self.observables = 0
        self.screens = 0
        self.failures: list[str] = []
        self.coverage: set[str] = set()

    @staticmethod
    def _display(value: object) -> list[str]:
        if isinstance(value, bytes):
            return value.decode("utf-8", "backslashreplace").splitlines(keepends=True)
        if isinstance(value, tuple) and value and isinstance(value[0], tuple):
            return ["".join(cell or "·" for cell in row).rstrip() + "\n" for row in value]
        return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n").splitlines(
            keepends=True
        )

    def compare(self, name: str, typescript: object, rust: object, *, screen: bool = False) -> None:
        self.observables += 1
        if screen:
            self.screens += 1
        if typescript == rust:
            return
        self.failures.append(name)
        diff = "".join(
            difflib.unified_diff(
                self._display(typescript),
                self._display(rust),
                fromfile=f"{name}.typescript",
                tofile=f"{name}.rust",
            )
        )
        target = self.artifacts / "divergences" / f"{safe_name(name)}.diff"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(diff or f"TypeScript: {typescript!r}\nRust: {rust!r}\n", encoding="utf-8")
        print(f"DIVERGENCE {name}\n{diff}", file=sys.stderr)

    def require_coverage(self, required: Iterable[str]) -> None:
        missing = sorted(set(required) - self.coverage)
        self.compare("screen.key-coverage", [], missing)


class TerminalGrid:
    """Small ANSI terminal model for the sequences emitted by Bun and Ratatui."""

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
    raw = "\n".join(
        json.dumps(entry, ensure_ascii=False, separators=(",", ":"), sort_keys=True) for entry in entries
    )
    return normalize_bytes((raw + ("\n" if raw else "")).encode("utf-8"), roots)


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
log = pathlib.Path(os.environ["PARITY_PROCESS_LOG"])
log.parent.mkdir(parents=True, exist_ok=True)
with log.open("a", encoding="utf-8") as output:
    output.write(json.dumps({"command": name, "args": sys.argv[1:]}, ensure_ascii=False, separators=(",", ":")) + "\\n")
if name == "herdr-fake":
    if os.environ.get("PARITY_HERDR_FAIL") == "1" and sys.argv[1:2] == ["plugin"]:
        sys.stderr.write(os.environ.get("PARITY_HERDR_STDERR", "fake herdr failure") + "\\n")
        raise SystemExit(7)
    raise SystemExit(0)
reads = {"pbpaste", "wl-paste", "xclip-read", "xsel-read"}
writes = {"pbcopy", "wl-copy", "xclip-write", "xsel-write"}
mode = "read" if name in reads or (name == "xclip" and "-out" in sys.argv) or (name == "xsel" and "--output" in sys.argv) else "write"
data = sys.stdin.buffer.read() if mode == "write" else b""
if mode == "write" and os.environ.get("PARITY_CLIPBOARD_OUTPUT"):
    pathlib.Path(os.environ["PARITY_CLIPBOARD_OUTPUT"]).write_bytes(data)
if os.environ.get("PARITY_CLIPBOARD_FAIL") in (mode, "all"):
    raise SystemExit(9)
if mode == "read":
    source = os.environ.get("PARITY_CLIPBOARD_INPUT")
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
    def __init__(self, root: Path, rust_binary: Path, workspace: Path, proof: Proof) -> None:
        self.root = root
        self.rust_binary = rust_binary
        self.workspace = workspace
        self.proof = proof
        self.fake_bin = create_fakes(workspace / "fake-bin")

    def command(self, implementation: str, entrypoint: str) -> list[str]:
        if implementation == "rust":
            return [str(self.rust_binary), entrypoint]
        scripts = {
            "capture": "capture.ts",
            "copy-context": "export.ts",
            "manage": "open-manager.ts",
            "editor": "editor.ts",
            "manager": "manager.ts",
        }
        return ["bun", str(self.root / "src" / scripts[entrypoint])]

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
                "PARITY_PROCESS_LOG": str(process_log),
                "PARITY_CLIPBOARD_INPUT": str(clipboard_input),
                "PARITY_CLIPBOARD_OUTPUT": str(clipboard_output),
                "TZ": "UTC",
                "LANG": "en_US.UTF-8",
                "LC_ALL": "en_US.UTF-8",
                "TERM": "xterm-256color",
            }
        )
        for key in ("PARITY_CLIPBOARD_FAIL", "PARITY_HERDR_FAIL", "PARITY_HERDR_STDERR", "HERDR_ANNOTATE_PENDING"):
            env.pop(key, None)
        if extra:
            env.update(extra)
        return env

    def run(self, implementation: str, entrypoint: str, env: Mapping[str, str]) -> CommandResult:
        result = subprocess.run(
            self.command(implementation, entrypoint),
            cwd=self.root,
            env=dict(env),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        return CommandResult(result.returncode, result.stdout, result.stderr)

    def process_pair(
        self,
        name: str,
        entrypoint: str,
        setup: Callable[[str, Path, Path, Path, Path, Path], Mapping[str, str] | None],
        inspect: Callable[[str, Path, Path, Path, Path, Path], object] | None = None,
    ) -> tuple[Path, Path]:
        case = self.workspace / name
        results: dict[str, tuple[CommandResult, Path, Path, Path, Path, Path]] = {}
        for implementation in ("typescript", "rust"):
            base = case / implementation
            state = base / "state"
            runtime = base / "runtime"
            log = base / "process.jsonl"
            clipboard_input = base / "clipboard-input"
            clipboard_output = base / "clipboard-output"
            base.mkdir(parents=True)
            runtime.mkdir()
            clipboard_input.write_bytes(b"")
            extra = setup(implementation, state, runtime, log, clipboard_input, clipboard_output) or {}
            env = self.environment(state, runtime, log, clipboard_input, clipboard_output, extra)
            results[implementation] = (
                self.run(implementation, entrypoint, env),
                state,
                runtime,
                log,
                clipboard_input,
                clipboard_output,
            )
        ts, rs = results["typescript"], results["rust"]
        roots = [ts[1], ts[2], rs[1], rs[2], self.workspace]
        self.proof.compare(f"{name}.exit", ts[0].exit_code, rs[0].exit_code)
        self.proof.compare(f"{name}.stdout", normalize_bytes(ts[0].stdout, roots), normalize_bytes(rs[0].stdout, roots))
        self.proof.compare(f"{name}.stderr", normalize_bytes(ts[0].stderr, roots), normalize_bytes(rs[0].stderr, roots))
        self.proof.compare(f"{name}.processes", read_process_log(ts[3], roots), read_process_log(rs[3], roots))
        if inspect:
            self.proof.compare(
                f"{name}.artifact",
                inspect("typescript", *ts[1:]),
                inspect("rust", *rs[1:]),
            )
        return ts[1], rs[1]

    def pty_pair(
        self,
        name: str,
        entrypoint: str,
        steps: Sequence[Step],
        marker: str,
        rows: int,
        cols: int,
        seed: Callable[[Path], None],
        extra: Mapping[str, str]
        | Callable[[str, Path], Mapping[str, str]]
        | None = None,
        compare_state: bool = False,
        compare_clipboard: bool = False,
    ) -> tuple[Path, Path]:
        case = self.workspace / name
        results: dict[str, tuple[PtyResult, Path, Path, Path, Path]] = {}
        for implementation in ("typescript", "rust"):
            base = case / implementation
            state = base / "state"
            runtime = base / "runtime"
            log = base / "process.jsonl"
            clipboard_input = base / "clipboard-input"
            clipboard_output = base / "clipboard-output"
            base.mkdir(parents=True)
            runtime.mkdir()
            clipboard_input.write_bytes(b"clipboard selection")
            seed(state)
            case_extra = extra(implementation, state) if callable(extra) else extra
            env = self.environment(
                state, runtime, log, clipboard_input, clipboard_output, case_extra
            )
            session = PtySession(self.command(implementation, entrypoint), env, self.root, rows, cols)
            session.wait_for(marker)
            screens = [("initial", session.grid.snapshot())]
            for step in steps:
                self.proof.coverage.update(step.coverage)
                if step.process_signal is None:
                    session.send(step.data)
                else:
                    session.send_signal(step.process_signal)
                screens.append((step.label, session.grid.snapshot()))
            code = session.finish()
            results[implementation] = (PtyResult(screens, code), state, runtime, log, clipboard_output)
        ts, rs = results["typescript"], results["rust"]
        roots = [ts[1], ts[2], rs[1], rs[2], self.workspace]
        self.proof.compare(f"{name}.exit", ts[0].exit_code, rs[0].exit_code)
        self.proof.compare(f"{name}.screen-count", len(ts[0].screens), len(rs[0].screens))
        for (ts_label, ts_screen), (rs_label, rs_screen) in zip(ts[0].screens, rs[0].screens):
            self.proof.compare(f"{name}.screen.{ts_label}.label", ts_label, rs_label)
            self.proof.compare(f"{name}.screen.{ts_label}", ts_screen, rs_screen, screen=True)
        self.proof.compare(f"{name}.processes", read_process_log(ts[3], roots), read_process_log(rs[3], roots))
        if compare_clipboard:
            self.proof.compare(
                f"{name}.clipboard",
                normalize_bytes(ts[4].read_bytes() if ts[4].exists() else b"", roots),
                normalize_bytes(rs[4].read_bytes() if rs[4].exists() else b"", roots),
            )
        if compare_state:
            self.proof.compare(f"{name}.state", state_snapshot(ts[1], roots), state_snapshot(rs[1], roots))
        return ts[1], rs[1]


def pending_artifact(_implementation: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> object:
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


def pending_and_runtime_artifact(
    implementation: str,
    state: Path,
    runtime: Path,
    log: Path,
    source: Path,
    sink: Path,
) -> object:
    return {
        "pending": pending_artifact(implementation, state, runtime, log, source, sink),
        "runtime": state_snapshot(runtime, [runtime]).decode("utf-8"),
    }


def clipboard_artifact(_implementation: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> object:
    del state, runtime, log, source
    return sink.read_bytes() if sink.exists() else b""


def no_pending_artifact(_implementation: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> object:
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

    def capture_context(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> Mapping[str, str]:
        del log, sink
        state.mkdir()
        handoff = handoff_path(runtime)
        handoff.parent.mkdir()
        handoff.write_text("lower-priority handoff", encoding="utf-8")
        source.write_text("lower-priority clipboard", encoding="utf-8")
        return {"HERDR_PLUGIN_CONTEXT_JSON": context}

    harness.process_pair(
        "process.capture.context", "capture", capture_context, pending_and_runtime_artifact
    )

    def capture_handoff(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del log, sink
        state.mkdir()
        handoff = handoff_path(runtime)
        handoff.parent.mkdir()
        handoff.write_text("handoff selection\n", encoding="utf-8")
        source.write_text("lower-priority clipboard", encoding="utf-8")

    harness.process_pair(
        "process.capture.handoff", "capture", capture_handoff, pending_and_runtime_artifact
    )

    def capture_stale_handoff(
        _impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> None:
        del log, sink
        state.mkdir()
        handoff = handoff_path(runtime)
        handoff.parent.mkdir()
        handoff.write_text("stale handoff", encoding="utf-8")
        stale = time.time() - 16
        os.utime(handoff, (stale, stale))
        source.write_text("clipboard after stale handoff", encoding="utf-8")

    harness.process_pair(
        "process.capture.stale-handoff",
        "capture",
        capture_stale_handoff,
        pending_and_runtime_artifact,
    )

    def capture_blank_handoff(
        _impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> None:
        del log, sink
        state.mkdir()
        handoff = handoff_path(runtime)
        handoff.parent.mkdir()
        handoff.write_text(" \n\t", encoding="utf-8")
        source.write_text("clipboard after blank handoff", encoding="utf-8")

    harness.process_pair(
        "process.capture.blank-handoff",
        "capture",
        capture_blank_handoff,
        pending_and_runtime_artifact,
    )

    def capture_invalid_utf8_handoff(
        _impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> None:
        del log, sink
        state.mkdir()
        handoff = handoff_path(runtime)
        handoff.parent.mkdir()
        handoff.write_bytes(b"invalid-\xff-handoff")
        source.write_text("clipboard after invalid handoff", encoding="utf-8")

    harness.process_pair(
        "process.capture.invalid-utf8-handoff",
        "capture",
        capture_invalid_utf8_handoff,
        pending_and_runtime_artifact,
    )

    def capture_clipboard(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, sink
        state.mkdir()
        source.write_bytes("clipboard 한 selection".encode("utf-8"))

    harness.process_pair("process.capture.clipboard", "capture", capture_clipboard, pending_artifact)

    def capture_invalid_context(
        _impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> Mapping[str, str]:
        del runtime, log, sink
        state.mkdir()
        source.write_text("clipboard after invalid context", encoding="utf-8")
        return {"HERDR_PLUGIN_CONTEXT_JSON": "{broken"}

    harness.process_pair(
        "process.capture.invalid-context", "capture", capture_invalid_context, pending_artifact
    )

    def capture_empty(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, sink
        state.mkdir()
        source.write_bytes(b" \n\t")

    harness.process_pair("process.capture.empty", "capture", capture_empty, no_pending_artifact)

    def capture_no_clipboard(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> Mapping[str, str]:
        del runtime, log, source, sink
        state.mkdir()
        return {"PARITY_CLIPBOARD_FAIL": "read"}

    harness.process_pair("process.capture.no-clipboard", "capture", capture_no_clipboard, no_pending_artifact)

    def capture_open_failure(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> Mapping[str, str]:
        del runtime, log, source, sink
        state.mkdir()
        return {
            "HERDR_PLUGIN_CONTEXT_JSON": context,
            "PARITY_HERDR_FAIL": "1",
            "PARITY_HERDR_STDERR": "pane open failed",
        }

    harness.process_pair("process.capture.open-failure", "capture", capture_open_failure, no_pending_artifact)

    def capture_missing_state(
        _impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> Mapping[str, str]:
        del state, runtime, log, source, sink
        return {"HERDR_PLUGIN_STATE_DIR": "", "HERDR_PLUGIN_CONTEXT_JSON": context}

    harness.process_pair("process.capture.missing-state", "capture", capture_missing_state)

    def capture_missing_root(
        _impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> Mapping[str, str]:
        del runtime, log, source, sink
        state.mkdir()
        return {"HERDR_PLUGIN_ROOT": "", "HERDR_PLUGIN_CONTEXT_JSON": context}

    harness.process_pair("process.capture.missing-root", "capture", capture_missing_root)

    def copy_empty(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del state, runtime, log, source, sink

    harness.process_pair(
        "process.copy.empty",
        "copy-context",
        copy_empty,
        lambda impl, state, runtime, log, source, sink: state_snapshot(state, [state]),
    )

    def copy_populated(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, source, sink
        seed_stores(state, archives=[])

    harness.process_pair("process.copy.populated", "copy-context", copy_populated, clipboard_artifact)

    def copy_single(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, source, sink
        seed_stores(state, annotations=ANNOTATIONS[:1], archives=[])

    harness.process_pair("process.copy.single", "copy-context", copy_single, clipboard_artifact)

    def copy_no_clipboard(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> Mapping[str, str]:
        del runtime, log, source, sink
        seed_stores(state, archives=[])
        return {"PARITY_CLIPBOARD_FAIL": "write"}

    harness.process_pair("process.copy.no-clipboard", "copy-context", copy_no_clipboard, clipboard_artifact)

    def copy_invalid(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, source, sink
        state.mkdir()
        (state / "annotations.jsonl").write_text("{broken\n", encoding="utf-8")

    harness.process_pair("process.copy.invalid-store", "copy-context", copy_invalid)

    def copy_missing_state(
        _impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> Mapping[str, str]:
        del state, runtime, log, source, sink
        return {"HERDR_PLUGIN_STATE_DIR": ""}

    harness.process_pair("process.copy.missing-state", "copy-context", copy_missing_state)

    def fresh_lock(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, source, sink
        state.mkdir()
        (state / ".annotations.lock").mkdir()

    harness.process_pair(
        "process.copy.busy-lock",
        "copy-context",
        fresh_lock,
        lambda impl, state, runtime, log, source, sink: state_snapshot(state, [state]),
    )

    def stale_lock(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        fresh_lock(_impl, state, runtime, log, source, sink)
        stale = time.time() - 31
        os.utime(state / ".annotations.lock", (stale, stale))

    harness.process_pair(
        "process.copy.stale-lock",
        "copy-context",
        stale_lock,
        lambda impl, state, runtime, log, source, sink: state_snapshot(state, [state]),
    )

    def manage_success(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del state, runtime, log, source, sink

    harness.process_pair("process.manage.success", "manage", manage_success)

    def manage_failure(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> Mapping[str, str]:
        del state, runtime, log, source, sink
        return {"PARITY_HERDR_FAIL": "1", "PARITY_HERDR_STDERR": "manager open failed"}

    harness.process_pair("process.manage.failure", "manage", manage_failure)

    def manage_failure_without_stderr(
        _impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> Mapping[str, str]:
        del state, runtime, log, source, sink
        return {"PARITY_HERDR_FAIL": "1", "PARITY_HERDR_STDERR": ""}

    harness.process_pair(
        "process.manage.failure-without-stderr", "manage", manage_failure_without_stderr
    )

    def manage_missing_root(
        _impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path
    ) -> Mapping[str, str]:
        del state, runtime, log, source, sink
        return {"HERDR_PLUGIN_ROOT": ""}

    harness.process_pair("process.manage.missing-root", "manage", manage_missing_root)

    def editor_missing(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> None:
        del runtime, log, source, sink
        state.mkdir()

    harness.process_pair("process.editor.missing-pending", "editor", editor_missing)

    def editor_invalid(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> Mapping[str, str]:
        del runtime, log, source, sink
        state.mkdir()
        pending = state / "invalid-pending.json"
        pending.write_text('{"selectedText":"only"}\n', encoding="utf-8")
        return {"HERDR_ANNOTATE_PENDING": str(pending)}

    harness.process_pair("process.editor.invalid-pending", "editor", editor_invalid)

    def manager_missing(_impl: str, state: Path, runtime: Path, log: Path, source: Path, sink: Path) -> Mapping[str, str]:
        del state, runtime, log, source, sink
        return {"HERDR_PLUGIN_STATE_DIR": ""}

    harness.process_pair("process.manager.missing-state", "manager", manager_missing)


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


def pending_file_extra(_implementation: str, state: Path) -> Mapping[str, str]:
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


def run_screen_and_store_layer(harness: Harness) -> tuple[Path, Path]:
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
    editor_ts, editor_rs = harness.pty_pair(
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
    harness.pty_pair(
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
    harness.pty_pair(
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
    harness.pty_pair(
        "screen.editor.control-c",
        "editor",
        [Step("control-c", b"\x03", ("editor:ctrl-c",))],
        "Selected text",
        22,
        86,
        editor_seed,
        editor_context,
    )
    harness.pty_pair(
        "screen.editor.missing-state",
        "editor",
        [Step("char", b"x"), Step("save", b"\x13"), Step("escape", b"\x1b")],
        "Selected text",
        22,
        86,
        editor_seed,
        {**editor_context, "HERDR_PLUGIN_STATE_DIR": ""},
    )
    harness.pty_pair(
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
        Step("active-y-failure", b"y", ("manager:active:y",)),
        Step("active-c-failure", b"c", ("manager:active:c",)),
        Step("active-C-failure", b"C", ("manager:active:C",)),
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
        Step("archives-y-failure", b"y", ("manager:archives:y",)),
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
    harness.pty_pair(
        "screen.manager.all-views",
        "manager",
        manager_steps,
        "Annotations (",
        28,
        98,
        manager_seed,
        {"PARITY_CLIPBOARD_FAIL": "write"},
        compare_state=True,
        compare_clipboard=True,
    )
    harness.pty_pair(
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
    harness.pty_pair(
        "screen.manager.escape-active",
        "manager",
        [Step("active-escape", b"\x1b", ("manager:active:Esc",))],
        "Annotations (",
        28,
        98,
        manager_seed,
    )
    harness.pty_pair(
        "screen.manager.control-c-active",
        "manager",
        [Step("control-c", b"\x03", ("manager:active:Ctrl-C",))],
        "Annotations (",
        28,
        98,
        manager_seed,
    )
    harness.pty_pair(
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
    harness.pty_pair(
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
    harness.pty_pair(
        "screen.manager.sighup",
        "manager",
        [Step("sighup", process_signal=signal.SIGHUP)],
        "Annotations (",
        28,
        98,
        manager_seed,
    )
    harness.pty_pair(
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
        ("active-y-success", [Step("copy-one", b"y", ("manager:active:y",))]),
        ("active-c-success", [Step("copy-all", b"c", ("manager:active:c",))]),
        (
            "archives-y-success",
            [
                Step("to-archives", b"\t", ("manager:active:Tab",)),
                Step("copy-archive", b"y", ("manager:archives:y",)),
            ],
        ),
    ):
        harness.pty_pair(
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

    harness.pty_pair(
        "store.manager.copy-archive",
        "manager",
        [Step("copy-archive", b"C", ("manager:active:C",))],
        "Annotations (",
        28,
        98,
        manager_seed,
        compare_state=True,
        compare_clipboard=True,
    )

    harness.proof.require_coverage(EDITOR_REQUIRED | MANAGER_REQUIRED)
    return editor_ts, editor_rs


def cross_read(harness: Harness, typescript_state: Path, rust_state: Path) -> None:
    case = harness.workspace / "store.cross-read"
    outputs: dict[str, tuple[CommandResult, bytes, bytes]] = {}
    for name, implementation, state in (
        ("typescript-reads-rust", "typescript", rust_state),
        ("rust-reads-typescript", "rust", typescript_state),
    ):
        base = case / name
        runtime = base / "runtime"
        runtime.mkdir(parents=True)
        log = base / "process.jsonl"
        source = base / "clipboard-input"
        source.write_bytes(b"")
        sink = base / "clipboard-output"
        env = harness.environment(state, runtime, log, source, sink)
        result = harness.run(implementation, "copy-context", env)
        outputs[name] = (result, sink.read_bytes() if sink.exists() else b"", read_process_log(log, [state, runtime]))
    left = outputs["typescript-reads-rust"]
    right = outputs["rust-reads-typescript"]
    harness.proof.compare("store.cross-read.exit", left[0].exit_code, right[0].exit_code)
    harness.proof.compare("store.cross-read.stderr", left[0].stderr, right[0].stderr)
    harness.proof.compare("store.cross-read.markdown", left[1], right[1])
    harness.proof.compare("store.cross-read.processes", left[2], right[2])


def verify_error_catalog(root: Path, proof: Proof) -> None:
    pairs = {
        "HERDR_PLUGIN_STATE_DIR is not set": ("src/capture.ts", "rust/src/cli.rs"),
        "HERDR_PLUGIN_ROOT is not set": ("src/capture.ts", "rust/src/cli.rs"),
        "No supported clipboard reader is available": ("src/clipboard.ts", "rust/src/clipboard.rs"),
        "No supported clipboard writer is available": ("src/clipboard.ts", "rust/src/clipboard.rs"),
        "Missing pending annotation": ("src/editor.ts", "rust/src/editor.rs"),
        "Pending annotation is invalid": ("src/editor.ts", "rust/src/editor.rs"),
        "Write a comment before saving.": ("src/editor.ts", "rust/src/editor.rs"),
        "Plugin state directory is unavailable.": ("src/editor.ts", "rust/src/editor.rs"),
        "Nothing to copy.": ("src/manager-copy.ts", "rust/src/manager_copy.rs"),
        "Nothing to copy and archive.": ("src/archive-workflow.ts", "rust/src/archive_workflow.rs"),
        "No archive selected.": ("src/manager.ts", "rust/src/manager.rs"),
        "Unable to save annotation": ("src/store.ts", "rust/src/store.rs"),
        "Unable to update annotations": ("src/store.ts", "rust/src/store.rs"),
        "Unable to update archives": ("src/store.ts", "rust/src/store.rs"),
        "Unable to read": ("src/store.ts", "rust/src/store.rs"),
        "Unable to access": ("src/store.ts", "rust/src/store.rs"),
        "Unable to lock": ("src/store.ts", "rust/src/store.rs"),
        "are busy; try again.": ("src/store.ts", "rust/src/store.rs"),
        "Copied and archived, but active annotations remain:": (
            "src/manager.ts",
            "rust/src/manager.rs",
        ),
        "Annotations restored, but the archive remains:": (
            "src/manager.ts",
            "rust/src/manager.rs",
        ),
    }
    for message, (typescript_file, rust_file) in pairs.items():
        present = (
            message in (root / typescript_file).read_text(encoding="utf-8"),
            message in (root / rust_file).read_text(encoding="utf-8"),
        )
        proof.compare(
            f"errors.catalog.{safe_name(message)}",
            (True, True),
            present,
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--rust-binary", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    os.umask(0o022)
    workspace = Path(tempfile.mkdtemp(prefix="herdr-annotate-parity-"))
    proof = Proof(workspace / "artifacts")
    try:
        harness = Harness(args.root.resolve(), args.rust_binary.resolve(), workspace, proof)
        print("== process layer")
        run_process_layer(harness)
        print("== screen and store layers")
        typescript_state, rust_state = run_screen_and_store_layer(harness)
        print("== cross-read and error catalog")
        cross_read(harness, typescript_state, rust_state)
        verify_error_catalog(args.root.resolve(), proof)
        if proof.failures:
            print(
                f"Parity Lite: {proof.observables} observables compared, {proof.screens} screens diffed, "
                f"{len(proof.failures)} divergences / "
                f"{len(DELIBERATE_DIVERGENCES)} deliberate; artifacts: {workspace}",
                file=sys.stderr,
            )
            return 1
        print(
            f"Parity Lite: {proof.observables} observables compared, {proof.screens} screens diffed, "
            f"zero divergences / {len(DELIBERATE_DIVERGENCES)} deliberate"
        )
        shutil.rmtree(workspace)
        return 0
    except Exception as error:  # noqa: BLE001 - harness must retain evidence on any failure.
        print(f"parity-lite: {error}; artifacts: {workspace}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
