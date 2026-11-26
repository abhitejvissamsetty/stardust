#!/usr/bin/env python3
"""Builds the CEF that Stardust uses, with H.264 and AAC played by the operating
system's decoders instead of FFmpeg's. See README.md beside this file.

    python3 cef-build/build.py download --dir ~/cef-build
    python3 cef-build/build.py build --dir ~/cef-build

`download` fetches CEF and Chromium without building, which needs only disk and
time. `build` adds Stardust's patch to CEF's patch list and builds a minimal
distribution, which needs a large computer. Then point Stardust's build at it:

    CEF_PATH=<the distribution folder> cargo xtask bundle --release
"""

import argparse
import os
import platform
import shutil
import subprocess
import sys
import urllib.request

# Must match the `cef` crate: CEF 158.0.6+g23f1736+chromium-158.0.8127.89.
CEF_BRANCH = "8127"
CEF_COMMIT = "23f1736"
AUTOMATE_URL = (
    "https://raw.githubusercontent.com/chromiumembedded/cef/"
    f"{CEF_COMMIT}/tools/automate/automate-git.py"
)

PATCH_NAME = "stardust_os_codecs"
HERE = os.path.dirname(os.path.abspath(__file__))

# Codecs: the H.264 and AAC parsers are built, but FFmpeg's decoders for them
# aren't (Chromium branding), and neither is the OpenH264 encoder or WebRTC's
# H.264. `use_os_proprietary_decoders` comes from Stardust's patch.
GN_DEFINES = [
    "is_official_build=true",
    "chrome_pgo_phase=0",
    "symbol_level=1",
    "proprietary_codecs=true",
    'ffmpeg_branding="Chromium"',
    "use_os_proprietary_decoders=true",
    "media_use_openh264=false",
    "rtc_use_h264=false",
]
# Linux has no system AAC decoder, so FFmpeg is built as its own libffmpeg.so
# that a person or a distribution can replace with one that has the codecs.
GN_DEFINES_LINUX = ["is_component_ffmpeg=true"]


def automate(directory, *args):
    script = os.path.join(directory, "automate-git.py")
    if not os.path.exists(script):
        urllib.request.urlretrieve(AUTOMATE_URL, script)
    command = [
        sys.executable,
        script,
        f"--download-dir={os.path.join(directory, 'src')}",
        f"--depot-tools-dir={os.path.join(directory, 'depot_tools')}",
        f"--branch={CEF_BRANCH}",
        f"--checkout={CEF_COMMIT}",
        "--no-chromium-history",
        *args,
    ]
    print("+", " ".join(command), flush=True)
    subprocess.run(command, check=True)


def add_patch(directory):
    """Adds Stardust's patch to CEF's list, which CEF applies before building.
    The download copies CEF into Chromium's tree, and the build uses that copy,
    so both get it."""
    for cef in (["src", "cef"], ["src", "chromium", "src", "cef"]):
        patch_dir = os.path.join(directory, *cef, "patch")
        if os.path.isdir(patch_dir):
            add_patch_to(patch_dir)


def add_patch_to(patch_dir):
    shutil.copy(
        os.path.join(HERE, "patches", f"{PATCH_NAME}.patch"),
        os.path.join(patch_dir, "patches", f"{PATCH_NAME}.patch"),
    )
    config = os.path.join(patch_dir, "patch.cfg")
    with open(config, encoding="utf-8") as file:
        text = file.read()
    if f"'{PATCH_NAME}'" in text:
        return
    end = text.rstrip().rfind("]")
    entry = (
        ",\n  {\n"
        "    # Stardust: H.264 and AAC through the operating system's decoders.\n"
        f"    'name': '{PATCH_NAME}',\n"
        "  }\n"
    )
    text = text[:end].rstrip() + entry + text[end:]
    with open(config, "w", encoding="utf-8") as file:
        file.write(text)


def build(directory):
    add_patch(directory)
    defines = GN_DEFINES + (GN_DEFINES_LINUX if sys.platform.startswith("linux") else [])
    os.environ["GN_DEFINES"] = " ".join(defines)
    arch = "--arm64-build" if platform.machine().lower() in ("arm64", "aarch64") else "--x64-build"
    # The checkout stays as downloaded: updating would reset CEF's patch list.
    automate(
        directory,
        "--no-update",
        "--force-build",
        "--no-debug-build",
        "--minimal-distrib",
        "--no-distrib-docs",
        arch,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("step", choices=["download", "build"])
    parser.add_argument("--dir", required=True, help="where CEF and Chromium go, about 100 GB")
    options = parser.parse_args()
    directory = os.path.abspath(os.path.expanduser(options.dir))
    os.makedirs(directory, exist_ok=True)
    if options.step == "download":
        automate(directory, "--no-build", "--no-distrib")
    else:
        build(directory)


if __name__ == "__main__":
    main()
