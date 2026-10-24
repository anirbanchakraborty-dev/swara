"""Measurement harness for the Skim refresh spike (docs/spikes/skim.md).

Run from the repository root with python3 -I:

    run.py open             build the test PDF, open it in Skim without focus, and compare the frontmost app
    run.py watch [runs]     rebuild the PDF with a different page count and time until Skim shows the new count,
                            read by a watcher that asks Skim every few milliseconds
    run.py revert [runs]    rebuild it, ask Skim to revert, and time the request
    run.py idle [runs]      time a page-count request when nothing has changed: the resolution of the others

Skim's preferences are read, never changed. Scratch output goes to $TMPDIR/swara-skim-spike.
"""

import os
import re
import statistics
import subprocess
import sys
import threading
import time

SCRATCH = os.path.join(os.environ.get("TMPDIR", "/tmp"), "swara-skim-spike")
PDF = os.path.join(SCRATCH, "view.pdf")
SHORT, LONG = 40, 80  # bar counts of the two songs the PDF alternates between
DEADLINE = 5.0  # seconds to wait for Skim to show a rebuilt PDF

# Runs inside one osascript process, so each page-count request costs about 3 ms rather than the
# 120 ms of starting osascript. Prints the wall-clock time and the count whenever the count changes.
WATCHER = """
ObjC.import("Foundation");
const skim = Application("Skim");
let last = -1;
while (true) {
  const n = skim.documents[0].pages.length;
  if (n !== last) { console.log(`${$.NSDate.date.timeIntervalSince1970} ${n}`); last = n; }
  delay(0.002);
}
"""


def sh(*cmd):
    return subprocess.run(cmd, check=True, capture_output=True, text=True).stdout.strip()


def front():
    """The frontmost application's LaunchServices identifier. Unlike System Events, lsappinfo needs no
    Automation permission."""
    return sh("lsappinfo", "front")


def build(bars):
    """Writes the song with this many bars and engraves it to the test PDF. Returns its page count and the
    moment LilyPond exited, which is when the PDF is complete."""
    ly = os.path.join(SCRATCH, "view.ly")
    with open(ly, "w") as f:
        f.write(sh("cargo", "xtask", "gen-song", "--bars", str(bars)) + "\n")
    subprocess.run(["lilypond", "-l", "WARNING", "-o", os.path.join(SCRATCH, "view"), ly],
                   check=True, capture_output=True, env={**os.environ, "LC_ALL": "C"})
    done = time.time()
    return int(re.search(r"^Pages:\s+(\d+)", sh("pdfinfo", PDF), re.M).group(1)), done


def skim(script):
    return sh("osascript", "-e", f'tell application "Skim" to {script}')


def skim_pages():
    return int(skim("count pages of document 1"))


class Watcher:
    """The JavaScript watcher above, with each change it reports as (wall-clock time, page count)."""

    def __init__(self):
        self.proc = subprocess.Popen(["osascript", "-l", "JavaScript", "-e", WATCHER],
                                     stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        self.changes = []
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.proc.stderr:
            at, n = line.split()
            self.changes.append((float(at), int(n)))

    def wait_for(self, want, since):
        """Seconds from `since` until Skim showed `want` pages, counting only changes after `since`'s
        build began, which the caller marks by the length of `changes` before it."""
        while time.time() - since < DEADLINE:
            hits = [at for at, n in self.changes[self.mark:] if n == want]
            if hits:
                return hits[0] - since
            time.sleep(0.01)
        raise SystemExit(f"Skim did not show {want} pages within {DEADLINE} s")

    def stop(self):
        self.proc.kill()
        self.proc.wait()


def focus_note(before):
    after = front()
    return "frontmost app unchanged" if after == before else f"frontmost app changed from {before} to {after}"


def ms_summary(label, secs):
    ms = sorted(s * 1000 for s in secs)
    print(f"{label}: median {statistics.median(ms):.0f} ms, min {ms[0]:.0f}, max {ms[-1]:.0f}, runs {len(ms)}")


def cmd_open():
    pages, _ = build(SHORT)
    before = front()
    t0 = time.perf_counter()
    subprocess.run(["open", "-g", "-a", "Skim", PDF], check=True)
    while subprocess.run(["pgrep", "-x", "Skim"], capture_output=True).returncode != 0:
        time.sleep(0.05)
    while True:
        try:
            if skim_pages() == pages:
                break
        except subprocess.CalledProcessError:
            time.sleep(0.05)
        if time.perf_counter() - t0 > 10:
            raise SystemExit("Skim did not open the PDF within 10 s")
    print(f"{pages} pages open in Skim {(time.perf_counter() - t0) * 1000:.0f} ms after open -g; {focus_note(before)}")


def cmd_watch(runs=5):
    watcher = Watcher()
    try:
        watcher.mark = 0
        short_pages, done = build(SHORT)
        watcher.wait_for(short_pages, done)
        secs = []
        for i in range(int(runs)):
            before = front()
            watcher.mark = len(watcher.changes)
            want, done = build(LONG if i % 2 == 0 else SHORT)
            written = os.stat(PDF).st_mtime
            secs.append(watcher.wait_for(want, done))
            print(f"run {i + 1}: {want} pages shown {secs[-1] * 1000:.0f} ms after LilyPond exited "
                  f"(PDF last written {(done - written) * 1000:.0f} ms before the exit); {focus_note(before)}")
        ms_summary("watch route", secs)
    finally:
        watcher.stop()


def cmd_revert(runs=5):
    secs = []
    for i in range(int(runs)):
        before = front()
        want, _ = build(LONG if i % 2 == 0 else SHORT)
        t0 = time.perf_counter()
        skim("revert document 1")
        secs.append(time.perf_counter() - t0)
        got = skim_pages()
        print(f"run {i + 1}: revert took {secs[-1] * 1000:.0f} ms, then Skim showed {got} of {want} pages; {focus_note(before)}")
    ms_summary("revert route", secs)


def cmd_idle(runs=10):
    secs = []
    for _ in range(int(runs)):
        t0 = time.perf_counter()
        skim_pages()
        secs.append(time.perf_counter() - t0)
    ms_summary("page-count request with nothing changed", secs)


if __name__ == "__main__":
    os.makedirs(SCRATCH, exist_ok=True)
    commands = {"open": cmd_open, "watch": cmd_watch, "revert": cmd_revert, "idle": cmd_idle}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        raise SystemExit(__doc__)
    commands[sys.argv[1]](*sys.argv[2:])
