"""Measurement harness for the warm playback spike (docs/spikes/playback.md).

Run from the repository root with python3 -I:

    run.py cold [runs]          launch-to-ready time of a FluidSynth player, over several launches
    run.py busy nc|fluidsynth   hold the first port with that program, then start a player anyway
    run.py stream <midi>        stream a MIDI file into a warm player through cargo xtask spike-play
    run.py offline <midi>       stream into a player that writes a WAV file, then measure the WAV
    run.py stop <midi> <ms>     the same, but stop after <ms>, then measure the silence
    run.py stops                compare candidate stop commands on a held organ note

Scratch output goes to $TMPDIR/swara-playback-spike.
"""

import math
import os
import re
import signal
import socket
import statistics
import struct
import subprocess
import sys
import threading
import time

SOUNDFONT = "/Applications/MuseScore 4.app/Contents/Resources/sound/MS Basic.sf3"
SCRATCH = os.path.join(os.environ.get("TMPDIR", "/tmp"), "swara-playback-spike")
BASE_PORT = 9800 + os.getpid() % 200
DEADLINE = 2.0  # seconds from launch to a confirmed player before the port is called busy
THRESHOLD = 32  # a sample above this magnitude (about -60 dBFS) counts as sound


class Player:
    """A FluidSynth process started by start_player, with its port and the stderr it has printed."""

    def __init__(self, proc, port, ready_after, stderr):
        self.proc, self.port, self.ready_after, self.stderr = proc, port, ready_after, stderr

    def stop(self):
        if self.proc.poll() is None:
            self.proc.send_signal(signal.SIGTERM)
            self.proc.wait(5)


def launch(port, extra, token=""):
    cmd = ["fluidsynth", "-s", "-i", "-o", f"shell.port={port}", "-o", f"shell.prompt={token}",
           "-o", "synth.dynamic-sample-loading=1", *extra, SOUNDFONT]
    proc = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
    lines = []
    threading.Thread(target=lambda: lines.extend(proc.stderr), daemon=True).start()
    return proc, lines


def handshake(port, token):
    """True when the program on the port greets a new connection with our token. FluidSynth sends its
    shell prompt the moment a client connects, and our player was launched with the token as its prompt,
    so another FluidSynth (whose prompt differs) and a program that is not FluidSynth both fail."""
    with socket.create_connection(("127.0.0.1", port), timeout=0.1) as s:
        s.settimeout(0.5)
        greeting = b""
        try:
            while len(greeting) < len(token):
                chunk = s.recv(256)
                if not chunk:
                    break
                greeting += chunk
        except socket.timeout:
            pass
        return greeting == token.encode()


def start_player(extra, base=BASE_PORT, tries=10, log=print):
    """The port strategy: try base, base+1, ... and keep the first port where our own player answers."""
    for port in range(base, base + tries):
        t0 = time.perf_counter()
        token = f"swara-{os.getpid()}-{port}>"
        proc, lines = launch(port, extra, token)
        verdict = None
        while verdict is None:
            if proc.poll() is not None:
                time.sleep(0.05)  # let the reader thread collect the last lines
                verdict = f"exited {proc.returncode}: {' / '.join(l.strip() for l in lines)}"
            elif any("bind" in l for l in lines):
                verdict = f"bind error: {' / '.join(l.strip() for l in lines)}"
            elif time.perf_counter() - t0 > DEADLINE:
                verdict = f"no answer within {DEADLINE} s"
            else:
                try:
                    if handshake(port, token):
                        ready = time.perf_counter() - t0
                        log(f"port {port}: ready after {ready * 1000:.0f} ms")
                        return Player(proc, port, ready, lines)
                    verdict = "connected, but the greeting was not our token: another program holds the port"
                except OSError:
                    time.sleep(0.01)
        log(f"port {port}: busy ({verdict})")
        if proc.poll() is None:
            proc.kill()
            proc.wait()
    raise SystemExit(f"no free port in {base}..{base + tries - 1}")


def spike_play(port, midi, *flags):
    cmd = ["cargo", "xtask", "spike-play", "--port", str(port), *flags, midi]
    out = subprocess.run(cmd, check=True, capture_output=True, text=True).stdout
    print(out, end="")
    return out


def read_wav(path):
    """Left-channel magnitudes and the sample rate. A killed FluidSynth never finalises the header, so the
    data chunk is read to the end of the file rather than by its length field."""
    data = open(path, "rb").read()
    fmt = data.find(b"fmt ")
    channels, rate = struct.unpack("<HI", data[fmt + 10 : fmt + 16])
    pcm = data[data.find(b"data") + 8 :]
    frames = len(pcm) // (2 * channels)
    samples = struct.unpack(f"<{frames * channels}h", pcm[: frames * channels * 2])
    return [abs(v) for v in samples[::channels]], rate


def loud_span(mags, rate):
    loud = [i for i, v in enumerate(mags) if v > THRESHOLD]
    return (loud[0] / rate, loud[-1] / rate) if loud else (None, None)


def cmd_cold(runs=5):
    times = []
    for _ in range(int(runs)):
        player = start_player(["-g", "0"])
        times.append(player.ready_after * 1000)
        player.stop()
    print(f"ready after: median {statistics.median(times):.0f} ms, min {min(times):.0f}, max {max(times):.0f}, runs {len(times)}")


def cmd_busy(holder):
    if holder == "nc":
        held = subprocess.Popen(["nc", "-l", str(BASE_PORT)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(0.3)
    else:
        held, _ = launch(BASE_PORT, ["-g", "0"])
        time.sleep(DEADLINE)
    print(f"holding {BASE_PORT} with {holder}")
    try:
        player = start_player(["-g", "0"])
        print(f"player started on {player.port}")
        player.stop()
    finally:
        held.kill()
        held.wait()


def cmd_stream(midi):
    player = start_player(["-g", "0"])  # Core Audio output, silent, so the speakers stay quiet
    try:
        spike_play(player.port, midi)
    finally:
        player.stop()


def cmd_offline(midi):
    wav = os.path.join(SCRATCH, "offline.wav")
    player = start_player(["-a", "file", "-o", f"audio.file.name={wav}"])
    try:
        out = spike_play(player.port, midi)
        time.sleep(5)  # the release and reverb tail
    finally:
        player.stop()
    first, last = (float(x) for x in re.search(r"first noteon at ([\d.]+) s, last event at ([\d.]+) s", out).groups())
    start, end = loud_span(*read_wav(wav))
    midi_len, sound_len = last - first, end - start
    print(f"MIDI length {midi_len:.3f} s, non-silent length {sound_len:.3f} s, difference {(sound_len / midi_len - 1) * 100:+.2f}%")


def cmd_stop(midi, ms):
    wav = os.path.join(SCRATCH, "stop.wav")
    player = start_player(["-a", "file", "-o", f"audio.file.name={wav}"])
    try:
        out = spike_play(player.port, midi, "--stop-after", ms)
        time.sleep(1)
    finally:
        player.stop()
    stop_after = float(re.search(r"stop written ([\d.]+) ms after the first noteon", out).group(1)) / 1000
    start, end = loud_span(*read_wav(wav))
    # The first sound marks the first note-on, so the stop reached the synth at start + stop_after.
    print(f"sound starts at {start:.4f} s, stop at {start + stop_after:.4f} s, last sound at {end:.4f} s, "
          f"silent {(end - start - stop_after) * 1000:.1f} ms after the stop")


def cmd_stops():
    """Each candidate stop, sent one second into a held organ note, the hardest case for a stop."""
    every = range(16)
    candidates = [
        ("reset", ["reset"]),
        ("gain 0, then reset", ["gain 0", "reset"]),
        ("cc 7 0 on every channel, then reset", [f"cc {c} 7 0" for c in every] + ["reset"]),
        ("reverb off, then reset", ["set synth.reverb.active 0", "reset"]),
        ("reverb off, cc 120 on every channel", ["set synth.reverb.active 0"] + [f"cc {c} 120 0" for c in every]),
        ("cc 120 on every channel", [f"cc {c} 120 0" for c in every]),
    ]
    for label, commands in candidates:
        wav = os.path.join(SCRATCH, "stops.wav")
        player = start_player(["-a", "file", "-o", f"audio.file.name={wav}"], log=lambda _: None)
        with socket.create_connection(("127.0.0.1", player.port)) as s:
            s.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            s.sendall(b"prog 0 19\n")  # program 19 from zero, General MIDI's church organ, which holds its level until released
            time.sleep(0.3)
            on = time.perf_counter()
            s.sendall(b"noteon 0 60 100\n")
            while time.perf_counter() - on < 1.0:
                time.sleep(0.0005)
            gap = time.perf_counter() - on
            s.sendall("".join(c + "\n" for c in commands).encode())
            time.sleep(1.0)
        player.stop()
        mags, rate = read_wav(wav)
        stop = int((loud_span(mags, rate)[0] + gap) * rate)
        before = max(mags[stop - rate // 10 : stop])
        window = rate // 100

        def quiet_after(level):
            # milliseconds from the stop until every later sample stays at or below the level, in 10 ms steps
            tail = [0] * (len(mags) + 1)
            for i in range(len(mags) - 1, -1, -1):
                tail[i] = max(mags[i], tail[i + 1])
            return next(((i - stop) * 1000 // rate for i in range(stop, len(mags), window) if tail[i] <= level), None)

        level = 20 * math.log10(before / 32768)
        print(f"{label:38s} {level:6.1f} dBFS held; 20 dB down after {quiet_after(before / 10)} ms; "
              f"below -60 dBFS after {quiet_after(THRESHOLD)} ms")


if __name__ == "__main__":
    os.makedirs(SCRATCH, exist_ok=True)
    commands = {"cold": cmd_cold, "busy": cmd_busy, "stream": cmd_stream, "offline": cmd_offline, "stop": cmd_stop, "stops": cmd_stops}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        raise SystemExit(__doc__)
    commands[sys.argv[1]](*sys.argv[2:])
