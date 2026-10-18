#!/usr/bin/env python3
"""`make cold-start`: a player's first launch, N times over (a pre-release
check).

Each start runs the game windowed from a release build with Godot's shader
cache and its whole user folder empty: HOME is a fresh scratch folder for
it, so the user data (the shader cache in it) starts empty, as on a first
launch, and nobody else's is touched. Headless runs compile no shaders and
can never meet what a cold start meets (a start that hung under its veil,
the shader cache empty: 3eea88f). A start passes when the `title` self-test
does within --timeout seconds (the title up over its scene). One that hangs
is sampled first (every thread's stack: macOS `sample`, or gdb), then
killed, and the check fails with where each thread waits. Apple's own Metal
shader cache is the machine's and stays warm.

The project is staged as tools/perf.py stages it (build/cold-start/project,
loading the release library).

    tools/cold_start.py                  # build, stage, five starts
    tools/cold_start.py --runs 10 --no-build
"""

import argparse
import os
import platform
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import perf  # noqa: E402  (the build and the staging are perf's)


def say(*a):
    print("cold-start:", *a, flush=True)


def sample(pid, run_dir):
    """Every thread's stack of a hung process, in run_dir/sample.txt (None
    when no sampler is at hand)."""
    out = run_dir / "sample.txt"
    if platform.system() == "Darwin":
        cmd = ["sample", str(pid), "3", "-file", str(out)]
        subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    elif shutil.which("gdb"):
        with open(out, "w") as f:
            subprocess.run(
                ["gdb", "-p", str(pid), "-batch", "-ex", "thread apply all bt"],
                stdout=f,
                stderr=subprocess.STDOUT,
            )
    return out if out.exists() and out.stat().st_size > 0 else None


def thread_name(header):
    """A sample's thread, as its header names it."""
    name = header.strip().lstrip(":").strip()
    if "com.apple.main-thread" in name:
        return "the main thread"
    return name or "a thread"


def waits(path):
    """Where each thread of a macOS sample stands: (thread, its last frame)."""
    threads, name, last = [], None, None
    head = re.compile(r"^\s+\d+ Thread_\d+\s*(.*)$")
    for line in path.read_text(errors="replace").splitlines():
        m = head.match(line)
        if m or not line.strip() or line.startswith(("Total number", "Sort by")):
            if name is not None and last:
                threads.append((name, last))
            name, last = (thread_name(m.group(1)) if m else None), None
            continue
        if name is not None:
            frame = re.sub(r"^[\s+!:|]*\d+\s+", "", line).strip()
            if frame:
                last = re.sub(r"\s+\[0x[0-9a-f]+\]$", "", frame)
    return threads


def start(i, stage_dir, godot, out, args):
    """One start from an empty user folder: (passed, what was seen)."""
    run = out / "runs" / str(i)
    shutil.rmtree(run, ignore_errors=True)
    home = run / "home"
    home.mkdir(parents=True)
    env = dict(os.environ)
    # HOME alone says where the user data lies (XDG_* would on Linux)
    for k in ("XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME"):
        env.pop(k, None)
    for k in ("RENETHACK_VARIANTS", "RENETHACK_VEIL_CAP", "RENETHACK_FRAME_LOG"):
        env.pop(k, None)
    env["HOME"] = str(home)
    env["RENETHACK_FRAME_STATS"] = "1"
    env.setdefault("RENETHACK_ENGINE_DIR", str(perf.ROOT / "engine" / "build"))
    cmd = [
        godot,
        "--path",
        str(stage_dir),
        *([] if args.vsync else ["--disable-vsync"]),
        "--",
        "--selftest=title",
        f"--size={args.size}",
        f"--playground={run / 'playground'}",
    ]
    began = time.monotonic()
    with open(run / "godot.log", "w") as log:
        p = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT, env=env)
        try:
            status = p.wait(timeout=args.timeout)
        except subprocess.TimeoutExpired:
            stacks = sample(p.pid, run)
            p.kill()
            p.wait()
            return False, ("hung", stacks, run)
    secs = time.monotonic() - began
    text = (run / "godot.log").read_text(errors="replace")
    if status == 0 and "SELFTEST PASS title" in text:
        found = lambda pat: (re.search(pat, text) or [None, "?"])[1]  # noqa: E731
        return True, (
            f"the title in {secs:.0f} s (the veil lifted at "
            f"{found(r'veil lifts ([0-9.]+) s after start')} s, the rehearsal "
            f"over at {found(r'rehearsal over ([0-9.]+) s after start')} s)"
        )
    return False, ("failed", status, run)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--timeout", type=float, default=240, help="seconds a start may take to the title")
    ap.add_argument("--out", default=str(perf.ROOT / "build" / "cold-start"))
    ap.add_argument("--godot")
    ap.add_argument("--size", default="1920x1080")
    ap.add_argument("--vsync", action="store_true", help="vsync on (off by default, as make perf: a quicker race)")
    ap.add_argument("--no-build", action="store_true")
    ap.add_argument("--lib", help="stage this extension library instead of building one")
    args = ap.parse_args()
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    godot = perf.godot_binary(args.godot)
    if not args.no_build and not args.lib:
        perf.build()
    stage_dir = perf.stage(out, godot, args.lib)
    failed = 0
    for i in range(1, args.runs + 1):
        load = perf.load_average()
        passed, seen = start(i, stage_dir, godot, out, args)
        if passed:
            say(f"start {i}: {seen}, load {load:.1f}")
            continue
        failed += 1
        if seen[0] == "hung":
            _, stacks, run = seen
            say(f"start {i}: HUNG, no title after {args.timeout:.0f} s (log: {run / 'godot.log'})")
            if stacks is None:
                say("  no stack sampler here (macOS sample, or gdb)")
            else:
                say(f"  every thread's stack: {stacks}")
                if platform.system() == "Darwin":
                    for name, frame in waits(stacks):
                        print(f"    {name}: {frame}")
        else:
            _, status, run = seen
            say(f"start {i}: FAILED (status {status}); the log's end ({run / 'godot.log'}):")
            for line in (run / "godot.log").read_text(errors="replace").splitlines()[-15:]:
                print("    " + line)
    if failed:
        say(f"FAILED: {failed} of {args.runs} starts did not reach the title")
        sys.exit(1)
    say(f"{args.runs} of {args.runs} starts reached the title")


if __name__ == "__main__":
    main()
