# Milestone 0 results

Measured on 2026-10-30 and 2026-10-31: Arch Linux, X11, Intel HD Graphics 620, 7.7 GB RAM.

## Status

| Item | Result |
|---|---|
| Linux build and run | Works |
| Sandbox on Linux | Works: `chrome://sandbox` reports namespace and seccomp-BPF protection |
| Wayland | Works in a nested sway session with `--ozone-platform=wayland`. Not tried in a full Wayland desktop session |
| Windows | **Not run.** Code type-checks only |
| macOS | **Not run.** Code type-checks only. No app menu or signing yet |
| CI | Written for all three platforms, but **never run**, because the project isn't on GitHub yet |

## Comparison with Chrome

Compared with Chrome for Testing 156.0.8078.80. This browser uses CEF 156.0.14 (Chromium 156.0.8078.48).

RAM is total PSS across all processes. CPU is the percentage of one core, sampled for 10 seconds after 20 seconds of settling.

| Page | This browser RAM | Chrome RAM | This browser CPU | Chrome CPU |
|---|---|---|---|---|
| example.com | 320 MB | 442 MB | 0% | 0% |
| YouTube home | 610 MB | 657 MB | 21% | 37% |
| 1080p VP9 video, 800x450 | 374 MB | 513 MB | 153% | 174% |

Treat these as rough: each row is from one run, and repeat runs varied, especially on YouTube, whose CPU reached 80% in this browser in an earlier run. Chrome also has its full UI and a larger window, while this browser has no UI yet, so the gap is likely to shrink as the UI is built.

Installed size: 372 MB, against 405 MB for Chrome for Testing's unpacked folder.

Reproduce with `scripts/measure.sh`.

## Video

- **Formats:** VP9, Opus and AV1 work. **H.264, AAC and HEVC don't**, because stock CEF builds leave out proprietary codecs.
- **Hardware decoding:** with `intel-media-driver` loaded and VA-API flags on, CPU on the video test fell from 141% to 52%. The flags are `--enable-features=AcceleratedVideoDecodeLinuxGL,VaapiVideoDecoder --ignore-gpu-blocklist`. Without a VA-API driver, the logs show `vaInitialize failed` and decoding uses the CPU.

## Blockers

1. **H.264 and AAC:** many sites depend on them. Fixing this means building CEF from source with `proprietary_codecs=true ffmpeg_branding=Chrome`, and hosting those binaries. Those codecs can carry patent licensing obligations when distributed, so get legal advice before shipping them. System codec support is an alternative to investigate.
2. **Windows and macOS are unproven:** they need a CI run and someone to launch the app on each system.
3. **VA-API isn't on by default:** the flags above need to be turned on where the driver exists, after testing on more GPUs.
