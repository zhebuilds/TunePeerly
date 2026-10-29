# Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
# Generated sine waves only; no third-party audio or Python packages.
import argparse
import array
import math
import sys
import wave
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description="Generate a 60-second seek-test tone, rising a semitone every 15 seconds.")
    parser.add_argument("output", type=Path, help="New WAV file to create; existing files are never overwritten")
    args = parser.parse_args()
    rate = 22050
    seconds = 60
    samples = array.array("h")
    phase = 0.0
    for i in range(rate * seconds):
        position = i / rate
        segment = int(position // 15)
        frequency = 440 * 2 ** (segment / 12)
        within = position % 15
        fade = min(1.0, within / 0.01, (15 - within) / 0.01)
        samples.append(round(3000 * fade * math.sin(phase)))
        phase = (phase + 2 * math.pi * frequency / rate) % (2 * math.pi)
    if sys.byteorder != "little":
        samples.byteswap()
    with args.output.open("xb") as raw:
        with wave.open(raw, "wb") as wav:
            wav.setnchannels(1)
            wav.setsampwidth(2)
            wav.setframerate(rate)
            wav.writeframes(samples.tobytes())
    print("Created:", args.output.resolve())
    print("Mono PCM16, 22050 Hz, 60 seconds; low-amplitude synthetic tones.")
    for segment in range(4):
        print(f"{segment*15:02d}-{(segment+1)*15:02d}s: {440 * 2 ** (segment / 12):.2f} Hz")
    print("Use a low playback volume. This fixture does not prove relay playback or seeking works.")


if __name__ == "__main__":
    main()
