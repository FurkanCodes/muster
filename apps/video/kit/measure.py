"""Measures the sound kit with numpy: a track's tempo, its beat grid, its
first downbeat and its drop; each sound effect's length and where its
loudest peak lands. Decodes with ffmpeg. Prints JSON.

    python3 measure.py music TRACK.mp3 [--plot beats.png]
    python3 measure.py effects A.mp3 B.mp3 ...
"""

import json
import subprocess
import sys

import numpy as np

RATE = 22050
HOP = 512
FFT = 2048


def decode(path):
    raw = subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-i", path, "-ac", "1", "-ar", str(RATE), "-f", "f32le", "-"],
        check=True,
        capture_output=True,
    ).stdout
    return np.frombuffer(raw, dtype=np.float32).astype(np.float64)


def spectrogram(x):
    window = np.hanning(FFT)
    frames = 1 + (len(x) - FFT) // HOP
    index = np.arange(FFT)[None, :] + HOP * np.arange(frames)[:, None]
    return np.abs(np.fft.rfft(x[index] * window, axis=1))


def flux(spec):
    """Onset strength: how much the log spectrum rises, frame to frame."""
    logged = np.log1p(100 * spec)
    rise = np.maximum(0, np.diff(logged, axis=0)).sum(axis=1)
    rise = np.concatenate([[0], rise])
    smooth = np.convolve(rise, np.ones(16) / 16, mode="same")
    return np.maximum(0, rise - smooth)


def comb(onset, period, phase):
    at = np.arange(phase, len(onset) - 1, period)
    return onset[np.round(at).astype(int)].sum() / max(1, len(at))


def tempo(onset, low=60, high=200, prefer=(110, 125)):
    """The tempo whose beat grid the onsets fall on best, refined to 0.01 BPM."""
    frame_rate = RATE / HOP

    def score(bpm):
        period = 60 * frame_rate / bpm
        return max(comb(onset, period, phase) for phase in np.arange(0, period, 0.5))

    coarse = np.arange(low, high, 0.5)
    scores = np.array([score(b) for b in coarse])
    best = coarse[np.argmax(scores)]
    inside = (coarse >= prefer[0]) & (coarse <= prefer[1])
    preferred = coarse[inside][np.argmax(scores[inside])]
    # A track at 120 scores at 60 and 240 too: take the preferred band's
    # tempo where it scores nearly as well as the best.
    if scores[inside].max() >= 0.85 * scores.max():
        best = preferred
    fine = np.arange(best - 0.6, best + 0.6, 0.01)
    return float(fine[np.argmax([score(b) for b in fine])])


def measure_music(path, plot=None):
    x = decode(path)
    spec = spectrogram(x)
    onset = flux(spec)
    frame_rate = RATE / HOP
    bpm = tempo(onset)
    period = 60 * frame_rate / bpm
    phases = np.arange(0, period, 0.25)
    phase = phases[np.argmax([comb(onset, period, p) for p in phases])]
    beats = np.arange(phase, len(onset) - 1, period) / frame_rate
    # Loudness and low end per beat, to find where the music starts and
    # where it drops.
    bass = spec[:, : int(150 * FFT / RATE) + 1]
    kick = np.maximum(0, np.diff(np.log1p(100 * bass), axis=0)).sum(axis=1)
    kick = np.concatenate([[0], kick])
    at = np.round(beats * frame_rate).astype(int)
    rms = np.sqrt(np.convolve(x**2, np.ones(HOP) / HOP, mode="same"))
    span = 60 / bpm
    per_beat = np.array([rms[int(t * RATE) : int((t + span) * RATE)].mean() for t in beats])
    low = np.array([kick[a : a + int(period)].mean() for a in at])
    weight = per_beat * (low + 1e-9)
    # The drop: the beat after which the next four bars are loudest, in
    # loudness and low end together, against the four bars before.
    best, drop_beat = 0.0, None
    for i in range(16, len(beats) - 16):
        ratio = weight[i : i + 16].mean() / weight[i - 16 : i].mean()
        if ratio > best:
            best, drop_beat = ratio, i
    # A section comes in on a downbeat, so the drop says where bars start:
    # in house every beat has the same kick, and the kick cannot.
    started = int(np.argmax(per_beat > 0.25 * per_beat.max()))
    first = drop_beat % 4 if drop_beat is not None else 0
    while first < started:
        first += 4
    downbeats = beats[first::4]
    drop = None if drop_beat is None else float(beats[drop_beat])
    result = {
        "file": path,
        "length_s": round(len(x) / RATE, 3),
        "bpm": round(bpm, 2),
        "first_downbeat_s": round(float(beats[first]), 3),
        "drop_s": None if drop is None else round(drop, 3),
        "drop_strength": round(best, 2),
        "beats_s": [round(float(t), 3) for t in beats],
        "downbeats_s": [round(float(t), 3) for t in downbeats],
    }
    if plot:
        draw(x, result, plot)
    return result


def draw(x, result, path):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    t = np.arange(len(x)) / RATE
    figure, (axis, near) = plt.subplots(2, 1, figsize=(22, 9), dpi=110, height_ratios=[1, 1])
    step = max(1, len(x) // 200_000)
    axis.plot(t[::step], x[::step], color="#4b4f63", linewidth=0.4)
    for beat in result["beats_s"]:
        axis.axvline(beat, color="#8ad1e8", linewidth=0.35, alpha=0.6)
    for bar in result["downbeats_s"]:
        axis.axvline(bar, color="#2f7fbf", linewidth=0.9)
    axis.axvline(result["first_downbeat_s"], color="#3aa76d", linewidth=2.2, label="first downbeat")
    if result["drop_s"] is not None:
        axis.axvline(result["drop_s"], color="#f2504b", linewidth=2.6, label="drop")
    axis.set_xlim(0, t[-1])
    axis.set_xlabel("seconds")
    axis.set_title(
        f"{result['file'].split('/')[-1]}: {result['bpm']} BPM, first downbeat "
        f"{result['first_downbeat_s']} s, drop {result['drop_s']} s (thin: beats, blue: bars)"
    )
    axis.legend(loc="upper right")
    # Close up: eight seconds either side of the drop, every beat numbered.
    if result["drop_s"] is not None:
        drop = result["drop_s"]
        inside = (t > drop - 8) & (t < drop + 8)
        near.plot(t[inside], x[inside], color="#4b4f63", linewidth=0.3)
        for number, beat in enumerate(result["beats_s"]):
            if drop - 8 < beat < drop + 8:
                bar = beat in result["downbeats_s"]
                near.axvline(beat, color="#2f7fbf" if bar else "#8ad1e8", linewidth=1.4 if bar else 0.6)
                near.text(beat, near.get_ylim()[1] if False else 1.45, str(number), fontsize=7, ha="center")
        near.axvline(drop, color="#f2504b", linewidth=2.6)
        near.set_xlim(drop - 8, drop + 8)
        near.set_ylim(-1.55, 1.6)
        near.set_title("close up on the drop: beat numbers, bars in dark blue")
    figure.tight_layout()
    figure.savefig(path)


def measure_effect(path):
    x = decode(path)
    envelope = np.sqrt(np.convolve(x**2, np.ones(64) / 64, mode="same"))
    return {
        "file": path,
        "length_ms": round(1000 * len(x) / RATE),
        "peak_ms": round(1000 * int(np.argmax(envelope)) / RATE),
    }


if __name__ == "__main__":
    kind, *rest = sys.argv[1:]
    if kind == "music":
        plot = rest[rest.index("--plot") + 1] if "--plot" in rest else None
        print(json.dumps(measure_music(rest[0], plot)))
    else:
        print(json.dumps([measure_effect(path) for path in rest]))
