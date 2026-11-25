# Custom CEF build

Stock CEF can't play H.264 or AAC. This builds the same CEF that the `cef` crate uses (156.0.14, Chromium 156.0.8078.48) with those formats played by the operating system's decoders, as Firefox does. Stardust ships no H.264 or AAC decoder of its own. Why, and what was considered: [docs/remaining-work.md](../docs/remaining-work.md#7-h264-and-aac-video).

## What it changes

[`patches/stardust_os_codecs.patch`](patches/stardust_os_codecs.patch) adds one GN argument, `use_os_proprietary_decoders`. With it:

| | H.264 video | AAC audio |
|---|---|---|
| Windows | Hardware decoder through D3D11, already Chromium's first choice | Windows' Media Foundation AAC decoder, which Chromium otherwise uses only for xHE-AAC |
| macOS | VideoToolbox, already Chromium's first choice | AudioToolbox, likewise widened from xHE-AAC to all AAC |
| Linux | VA-API | Only through a replacement `libffmpeg.so` that has the codecs |

The other changes it needs:

- **FFmpeg is built with Chromium branding,** so its H.264 and AAC decoders aren't in the build. GN normally refuses `proprietary_codecs` with that branding, and the patch skips that check when `use_os_proprietary_decoders` is set.
- **Sample format:** without its AAC decoder, FFmpeg can't fill in AAC's sample format. Chromium then dropped the audio track, which is why AAC failed in [CEF issue #3559](https://github.com/chromiumembedded/cef/issues/3559). The patch fills it in, as Chromium already does for AC3.
- **ADTS headers:** Media Source Extensions put ADTS headers on AAC.
  - Windows asks for them on every stream, so plain files and streams arrive alike.
  - macOS strips them, because AudioToolbox takes raw AAC.
- **Build settings** also turn off the OpenH264 encoder and WebRTC's H.264 (`build.py`), since both are patented software encoders and decoders.

The patch is Stardust's own work, MPL-2.0. [Cromite](https://github.com/uazo/cromite/pull/856) does something similar for Windows and Android, but its patch is GPL-2.0, so none of it was copied.

## Building

Each system builds on its own: Linux on Linux, Windows on Windows, macOS on a Mac.

**Needs:**
- At least 16 GB RAM, with 32 GB or more recommended.
- About 200 GB of free disk.
- About 4 hours on 16 cores.
- Windows also needs Visual Studio and the Windows SDK; see [CEF's build guide](https://chromiumembedded.github.io/cef/master_build_quick_start).

```bash
python3 cef-build/build.py download --dir ~/cef-build
```

```bash
python3 cef-build/build.py build --dir ~/cef-build
```

On Debian or Ubuntu, run Chromium's `src/chromium/src/build/install-build-deps.sh` once between the two steps. The distribution lands in `~/cef-build/src/chromium/src/cef/binary_distrib/`.

Stardust then builds against it with no code changes:

```bash
CEF_PATH=~/cef-build/src/chromium/src/cef/binary_distrib/<folder> cargo xtask bundle --release
```

`cef-rs` reads `CEF_PATH`. To serve builds to CI instead, host the archives and set `CEF_DOWNLOAD_URL`.

## Status

Checked on Linux on 2026-11-25, without a full build:

- CEF applied all 118 of its patches with Stardust's, and GN generated the build with these settings.
- FFmpeg's settings for Windows, macOS and Linux have no H.264 or AAC decoder, but keep the MP4 demuxer and the code that reads AAC's sample rate and channels.
- `media/ffmpeg/ffmpeg_common.cc` compiles with the patch, which is the only patched file Linux builds. The Windows and macOS files can only be compiled on those systems.

Build on Debian or Ubuntu, as Chromium expects. On Arch, Chromium's Wayland tool failed to compile against Arch's newer XML library.

Not yet built in full or run. Check on each system:

1. An MP4 with H.264 and AAC plays with sound, from a plain `<video src>` and through Media Source Extensions (YouTube's `?app=desktop` player, or a Shaka Player demo).
2. HE-AAC plays at full quality on macOS. AudioToolbox may decode only its core at half the sample rate.
3. `chrome://media-internals` shows `MojoAudioDecoder` for AAC and a hardware decoder for H.264, and never `FFmpegAudioDecoder` or `FFmpegVideoDecoder` for either.

Every CEF security release needs this build again, on all three systems, and the patch rechecked against the new Chromium.
