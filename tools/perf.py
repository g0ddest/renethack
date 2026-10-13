#!/usr/bin/env python3
"""`make perf`: the game timed end to end on a release build.

It builds the extension in release and stages a copy of the Godot project
that loads it (build/perf/project: the files cloned, the library in bin/,
the project renamed so that its user data, Godot's shader cache among it,
is its own and starts empty). Then it runs scenes in a window at 1920x1080
with vsync off. Each run logs every frame (RENETHACK_FRAME_LOG,
client/rust/renethack-gd/src/frame_log.rs) while the process's resident
memory is sampled. The scenes: the tour, the branches, the rays and
effects, the item uses, a crowded room, soaks into the Mines and deeper,
and a long soak for leaks. It reports the frame times after the start-up
veil (p50, p95, p99, max), every frame over the budget with what it was
doing, and the memory over the long soak. It fails when a frame after the
veil goes over the budget.

Frame timings are worth nothing on a busy machine: it says the load it ran
at and will not judge above --max-load (unless --force). Apple's own Metal
shader cache is the machine's and stays warm between runs; Godot's is
emptied before each run unless --warm.

    tools/perf.py                      # build, stage, run everything, judge
    tools/perf.py --scenes tour,crowd  # only these
    tools/perf.py --no-build --report  # judge the logs of the last run again
"""

import argparse
import csv
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import threading
import time
from pathlib import Path

# the tree whose project and engine are staged (RENETHACK_ROOT: another
# checkout, e.g. an older commit's)
ROOT = Path(os.environ.get("RENETHACK_ROOT") or Path(__file__).resolve().parent.parent)
PROJECT = ROOT / "client" / "godot"
RUST = ROOT / "client" / "rust"
PERF_NAME = "renethack-perf"

# name: (self-test, more arguments, timeout in seconds)
SCENES = {
    "tour": ("tour", [], 300),
    "smoke": ("smoke", [], 300),
    "branches": ("branches", [], 600),
    "rays": ("rays", [], 300),
    "effects": ("effects", [], 300),
    "item-use": ("item-use", [], 300),
    "states": ("states", [], 400),
    "crowd": ("crowd", [], 400),
    # soaks whose random play reaches the Gnomish Mines (seeds 3 and 4)
    # and Dlvl 7 (seed 5)
    "soak-mines": ("soak", ["--soak=1200", "--seed=3"], 900),
    "soak-mines-2": ("soak", ["--soak=1200", "--seed=4"], 900),
    "soak-deep": ("soak", ["--soak=1200", "--seed=5"], 900),
    # the long soak, for the memory
    "soak-long": ("soak", ["--soak=2000", "--seed=1"], 1500),
}
DEFAULT_SCENES = [s for s in SCENES if s != "smoke"]


def say(*a):
    print("perf:", *a, flush=True)


def load_average():
    try:
        return os.getloadavg()[0]
    except OSError:
        return 0.0


def godot_binary(arg):
    if arg:
        return arg
    if os.environ.get("GODOT"):
        g = os.environ["GODOT"]
        if g.endswith(".app"):
            g = os.path.join(g, "Contents", "MacOS", "Godot")
        return g
    mac = "/Applications/Godot.app/Contents/MacOS/Godot"
    if os.path.exists(mac):
        return mac
    return shutil.which("godot") or "godot"


def library_name():
    system = platform.system()
    if system == "Darwin":
        return "librenethack_gd.dylib"
    if system == "Windows":
        return "renethack_gd.dll"
    return "librenethack_gd.so"


def target_dir():
    return Path(os.environ.get("CARGO_TARGET_DIR", RUST / "target"))


def build():
    say("building the extension in release")
    subprocess.run(
        ["cargo", "build", "--release", "-p", "renethack-gd"],
        cwd=RUST,
        check=True,
    )


def clone(src, dst):
    if platform.system() == "Darwin":
        subprocess.run(["cp", "-c", "-R", "-p", str(src), str(dst)], check=True)
    else:
        subprocess.run(["cp", "-a", "--reflink=auto", str(src), str(dst)], check=True)


def user_dir():
    """The staged project's user data (Godot's shader cache is in it)."""
    system = platform.system()
    if system == "Darwin":
        base = Path.home() / "Library" / "Application Support" / "Godot"
    elif system == "Windows":
        base = Path(os.environ.get("APPDATA", "")) / "Godot"
    else:
        base = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local" / "share")) / "godot"
    return base / "app_userdata" / PERF_NAME


def stage(out, godot, lib=None):
    """The project, loading the release library (or `lib`), under its own
    name."""
    stage_dir = out / "project"
    stage_dir.mkdir(parents=True, exist_ok=True)
    for f in stage_dir.iterdir():
        if f.name == ".godot":
            continue
        if f.is_dir() and not f.is_symlink():
            shutil.rmtree(f)
        else:
            f.unlink()
    if not (stage_dir / ".godot").exists() and (PROJECT / ".godot").exists():
        clone(PROJECT / ".godot", stage_dir)
    for f in PROJECT.iterdir():
        if f.name in (".godot", "project.godot", "renethack.gdextension", "export_presets.cfg"):
            continue
        clone(f, stage_dir)
    lib = Path(lib) if lib else target_dir() / "release" / library_name()
    if not lib.exists():
        sys.exit(f"perf: no release library at {lib} (run without --no-build)")
    (stage_dir / "bin").mkdir(exist_ok=True)
    shutil.copy2(lib, stage_dir / "bin" / library_name())
    ext = (PROJECT / "renethack.gdextension").read_text()
    for kind in ("debug", "release"):
        ext = ext.replace(f"res://../rust/target/{kind}/", "res://bin/")
    (stage_dir / "renethack.gdextension").write_text(ext)
    proj = (PROJECT / "project.godot").read_text()
    proj = proj.replace('config/name="renethack"', f'config/name="{PERF_NAME}"')
    (stage_dir / "project.godot").write_text(proj)
    say("importing the staged project")
    with open(out / "import.log", "w") as log:
        subprocess.run(
            [godot, "--headless", "--path", str(stage_dir), "--import"],
            stdout=log,
            stderr=subprocess.STDOUT,
        )
    return stage_dir


class Sampler(threading.Thread):
    """The resident memory of a process (and of its children: the engine)
    every half second, in MB."""

    def __init__(self, pid):
        super().__init__(daemon=True)
        self.pid = pid
        self.samples = []  # (seconds, godot MB, engine MB, load average)
        self.stop = threading.Event()
        self.t0 = time.monotonic()

    def run(self):
        while not self.stop.is_set():
            try:
                out = subprocess.run(
                    ["ps", "-A", "-o", "pid=,ppid=,rss="],
                    capture_output=True,
                    text=True,
                    timeout=5,
                ).stdout
            except (OSError, subprocess.TimeoutExpired):
                out = ""
            own = engine = 0
            for line in out.splitlines():
                parts = line.split()
                if len(parts) != 3:
                    continue
                pid, ppid, rss = (int(p) for p in parts)
                if pid == self.pid:
                    own = rss
                elif ppid == self.pid:
                    engine += rss
            if own:
                self.samples.append((time.monotonic() - self.t0, own / 1024, engine / 1024, load_average()))
            self.stop.wait(0.5)


def run_scene(name, stage_dir, godot, out, warm, size, stats=False, quality=None):
    test, extra, timeout = SCENES[name]
    run_dir = out / "runs" / name
    if run_dir.exists():
        shutil.rmtree(run_dir)
    run_dir.mkdir(parents=True)
    if not warm:
        shutil.rmtree(user_dir() / "shader_cache", ignore_errors=True)
    env = dict(os.environ)
    env["RENETHACK_FRAME_LOG"] = str(run_dir / "frames.csv")
    env.setdefault("RENETHACK_ENGINE_DIR", str(ROOT / "engine" / "build"))
    if quality:
        env["RENETHACK_QUALITY"] = quality
    if stats:
        env["RENETHACK_FRAME_STATS"] = "1"
    else:
        env.pop("RENETHACK_FRAME_STATS", None)
    cmd = [
        godot,
        "--path",
        str(stage_dir),
        "--disable-vsync",
        "--",
        f"--selftest={test}",
        f"--size={size}",
        f"--playground={run_dir / 'playground'}",
        *extra,
    ]
    say(f"{name}: {' '.join(cmd[4:])}")
    with open(run_dir / "godot.log", "w") as log:
        p = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT, env=env)
        sampler = Sampler(p.pid)
        sampler.start()
        try:
            status = p.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            p.kill()
            status = "timeout"
        sampler.stop.set()
        sampler.join()
    with open(run_dir / "memory.csv", "w") as f:
        f.write("t,godot_mb,engine_mb,load\n")
        for t, a, b, load in sampler.samples:
            f.write(f"{t:.1f},{a:.1f},{b:.1f},{load:.1f}\n")
    text = (run_dir / "godot.log").read_text(errors="replace")
    passed = f"SELFTEST PASS {test}" in text
    (run_dir / "status").write_text(f"{status} {'pass' if passed else 'fail'}\n")
    if not passed:
        say(f"{name}: the self-test did not pass (status {status}):")
        for line in text.splitlines()[-15:]:
            print("    " + line)
    return passed


def other_godots():
    """Godot processes running besides an idle editor."""
    out = subprocess.run(["pgrep", "-fl", "Godot"], capture_output=True, text=True).stdout
    return [p for p in out.splitlines() if "--editor" not in p and "pgrep" not in p]


def wait_quiet(max_load, secs):
    """Wait (up to `secs`) for the load to fall under `max_load`: the load
    then, and how many other Godot processes run."""
    deadline = time.monotonic() + secs
    while True:
        load, others = load_average(), len(other_godots())
        if load < max_load or time.monotonic() >= deadline:
            return load, others
        say(f"waiting for a quiet machine: load {load:.1f}, {others} other Godot")
        time.sleep(30)


def percentile(values, p):
    if not values:
        return 0.0
    s = sorted(values)
    k = (len(s) - 1) * p / 100
    lo = int(k)
    hi = min(lo + 1, len(s) - 1)
    return s[lo] + (s[hi] - s[lo]) * (k - lo)


def cause(row):
    """What a long frame did, as far as the log tells."""
    why = []
    if int(row["pipelines"]) > 0:
        why.append(f"{row['pipelines']} pipelines compiled")
    if int(row["built"]) > 0:
        why.append(f"{row['built']} models built")
    def ms(part):
        return float(row.get(part) or 0)

    for part, label in (
        ("map", "the map's sync"),
        ("views", "the views in all (the map, the HUD, the panels)"),
        ("pump", "the engine's messages"),
        ("input", "input/UI"),
        ("warm", "the title's warm-up"),
        ("preload", "the map's loading ahead"),
        ("test", "the self-test step"),
    ):
        if ms(part) >= 8:
            why.append(f"{label} {ms(part):.0f} ms")
    vmem = float(row["vmem"])
    if abs(vmem) >= 1:
        why.append(f"video memory {vmem:+.0f} MB")
    render = float(row.get("render") or 0)
    if render >= 8:
        why.append(f"the renderer's CPU {render:.0f} ms ({row.get('draws', '?')} draws)")
    rest = float(row["wall"]) - float(row["work"]) - render
    if rest >= 15:
        why.append(f"{rest:.0f} ms elsewhere (Godot, waiting on the GPU, the machine)")
    return "; ".join(why) or "nothing logged"


def report(out, scenes, budget):
    lines = []
    all_after = []
    over = []
    verdict = True
    summary = {}
    for name in scenes:
        run_dir = out / "runs" / name
        path = run_dir / "frames.csv"
        if not path.exists():
            lines.append(f"{name}: no frame log")
            verdict = False
            continue
        with open(path, newline="") as f:
            rows = list(csv.DictReader(f))
        after = [r for r in rows if r["veil"] == "0"]
        walls = [float(r["wall"]) for r in after]
        all_after += walls
        status = (run_dir / "status").read_text().strip() if (run_dir / "status").exists() else "?"
        if "pass" not in status.split():
            verdict = False
        long = [r for r in after if float(r["wall"]) > budget]
        over += [(name, r) for r in long]
        stats = {
            "frames": len(walls),
            "p50": percentile(walls, 50),
            "p95": percentile(walls, 95),
            "p99": percentile(walls, 99),
            "max": max(walls) if walls else 0.0,
            "over": len(long),
            "veiled_frames": len(rows) - len(after),
            "status": status,
        }
        mem = memory(run_dir)
        if mem:
            stats["memory"] = mem
        loads = run_loads(run_dir)
        if loads:
            stats["load"] = loads
        summary[name] = stats
        lines.append(
            f"{name:14} {stats['frames']:6} frames  p50 {stats['p50']:5.1f}  p95 {stats['p95']:5.1f}"
            f"  p99 {stats['p99']:5.1f}  max {stats['max']:6.1f} ms  over {budget:g} ms: {stats['over']}"
            f"  ({status})"
            + (f"  load {loads[0]:.1f}..{loads[1]:.1f}" if loads else "")
        )
    if all_after:
        lines.append(
            f"{'all':14} {len(all_after):6} frames  p50 {percentile(all_after, 50):5.1f}"
            f"  p95 {percentile(all_after, 95):5.1f}  p99 {percentile(all_after, 99):5.1f}"
            f"  max {max(all_after):6.1f} ms  over {budget:g} ms: {len(over)}"
        )
    if over:
        lines.append("")
        lines.append(f"frames over {budget:g} ms after the veil:")
        for name, r in over:
            lines.append(
                f"  {name} #{r['frame']} at {float(r['t']):.1f} s: {float(r['wall']):.1f} ms"
                f" (Dlvl {r['depth']} {r['branch']}, {r['state']}, step \"{r['step']}\"): {cause(r)}"
            )
        verdict = False
    lines.append("")
    lines.append("resident memory (MB): game shown, end, peak; the engine's peak; requests")
    for name in scenes:
        mem = summary.get(name, {}).get("memory")
        if mem:
            grow = mem.get("per_1000_requests")
            lines.append(
                f"  {name:14} {mem['start']:6.0f} {mem['end']:6.0f} {mem['peak']:6.0f}"
                f"   engine {mem['engine_peak']:4.0f}   {mem['requests']:5} requests"
                + (f"; second half {grow:+.1f} MB per 1000 requests" if grow is not None else "")
            )
    text = "\n".join(lines)
    (out / "report.txt").write_text(text + "\n")
    (out / "summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    print(text)
    return verdict and not over


def run_loads(run_dir):
    """The load average during a run: its mean and its most."""
    path = run_dir / "memory.csv"
    if not path.exists():
        return None
    with open(path, newline="") as f:
        loads = [float(r["load"]) for r in csv.DictReader(f) if r.get("load")]
    return (statistics.fmean(loads), max(loads)) if loads else None


def memory(run_dir):
    """The process's memory over a run: when the game showed, at its end, at
    most, and (over a long run) how it grew in its second half, a thousand
    of the engine's requests at a time."""
    path = run_dir / "memory.csv"
    frames = run_dir / "frames.csv"
    if not path.exists() or not frames.exists():
        return None
    with open(path, newline="") as f:
        rows = [(float(r["t"]), float(r["godot_mb"]), float(r["engine_mb"])) for r in csv.DictReader(f)]
    with open(frames, newline="") as f:
        log = [(float(r["t"]), int(r["requests"]), r["veil"], r["state"]) for r in csv.DictReader(f)]
    if len(rows) < 4 or not log:
        return None
    # the sampler's clock starts with the process, the log's a little later:
    # both are near enough for memory
    shown = next((t for t, _, veil, state in log if veil == "0" and state != "Title"), log[0][0])
    game = [r for r in rows if r[0] >= shown] or rows

    def requests_at(t):
        lo, hi = 0, len(log) - 1
        while lo < hi:
            mid = (lo + hi + 1) // 2
            if log[mid][0] <= t:
                lo = mid
            else:
                hi = mid - 1
        return log[lo][1]

    result = {
        "start": game[0][1],
        "end": game[-1][1],
        "peak": max(r[1] for r in game),
        "engine_peak": max(r[2] for r in rows),
        "requests": log[-1][1],
    }
    # growth per thousand requests over the second half of a long run
    if log[-1][1] >= 1000:
        half = game[len(game) // 2 :]
        xs = [requests_at(r[0]) / 1000 for r in half]
        ys = [r[1] for r in half]
        mx, my = statistics.fmean(xs), statistics.fmean(ys)
        var = sum((x - mx) ** 2 for x in xs)
        if var > 0:
            result["per_1000_requests"] = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / var
    return result


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--scenes", default=",".join(DEFAULT_SCENES), help="comma-separated: " + ", ".join(SCENES))
    ap.add_argument("--out", default=str(ROOT / "build" / "perf"))
    ap.add_argument("--godot")
    ap.add_argument("--budget", type=float, default=33.0, help="milliseconds a frame may take after the veil")
    ap.add_argument("--max-load", type=float, default=5.0)
    ap.add_argument("--wait", type=float, default=0, help="minutes to wait for a quiet machine before each scene")
    ap.add_argument("--force", action="store_true", help="judge even on a busy machine")
    ap.add_argument("--no-build", action="store_true")
    ap.add_argument("--warm", action="store_true", help="keep Godot's shader cache between runs")
    ap.add_argument("--report", action="store_true", help="only judge the logs already in --out")
    ap.add_argument("--size", default="1920x1080")
    ap.add_argument("--stats", action="store_true", help="the game's own frame statistics in the logs too")
    ap.add_argument("--quality", choices=["low", "medium", "high", "ultra"], help="the graphics preset (else the device's: High)")
    ap.add_argument("--keep-stage", action="store_true", help="run the project already staged in --out (its library as it is)")
    ap.add_argument("--lib", help="stage this extension library instead of building one")
    ap.add_argument("--stage-only", action="store_true", help="stage the project and stop")
    args = ap.parse_args()
    scenes = [s for s in args.scenes.split(",") if s]
    unknown = [s for s in scenes if s not in SCENES]
    if unknown:
        sys.exit(f"perf: no scene {', '.join(unknown)}")
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    godot = godot_binary(args.godot)
    if not args.report:
        if args.keep_stage:
            stage_dir = out / "project"
            if not (stage_dir / "project.godot").exists():
                sys.exit(f"perf: nothing staged in {stage_dir}")
        else:
            if not args.no_build and not args.lib:
                build()
            stage_dir = stage(out, godot, args.lib)
            if args.stage_only:
                say(f"staged {stage_dir}")
                return
        loads = []
        for name in scenes:
            load, others = wait_quiet(args.max_load, args.wait * 60)
            loads.append(load)
            say(f"{name}: load {load:.1f}" + (f", {others} other Godot running" if others else ""))
            run_scene(name, stage_dir, godot, out, args.warm, args.size, args.stats, args.quality)
        (out / "load.txt").write_text(" ".join(f"{l:.1f}" for l in loads) + "\n")
    ok = report(out, scenes, args.budget)
    means = [run_loads(out / "runs" / s) for s in scenes]
    means = [m[0] for m in means if m]
    worst = max(means, default=0.0)
    print(f"\nmean load during the runs: {', '.join(f'{m:.1f}' for m in means) or 'unknown'} (judged only under {args.max_load:g})")
    if worst > args.max_load and not args.force:
        print("perf: the machine was too busy for these timings to mean anything; run again when it is quiet")
        sys.exit(2)
    if not ok:
        print("perf: FAILED")
        sys.exit(1)
    print("perf: passed")


if __name__ == "__main__":
    main()
