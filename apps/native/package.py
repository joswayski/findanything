#!/usr/bin/env python3
"""Stage and package native builds with pinned Velopack. Never installs or publishes."""
import argparse
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
VPK_VERSION = "1.2.158"


def channel(system, machine):
    os_name = {"Darwin": "osx", "Windows": "win", "Linux": "linux"}[system]
    arch = {"x86_64": "x64", "AMD64": "x64", "arm64": "arm64", "aarch64": "arm64"}[machine]
    return f"preview-{os_name}-{arch}"


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--signed", action="store_true")
    parser.add_argument("--output", type=Path, default=ROOT / "releases")
    parser.add_argument("--stage", type=Path, default=ROOT / "staging")
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", args.version):
        parser.error("version must be major.minor.patch")
    if VPK_VERSION not in subprocess.check_output(["vpk", "--version"], text=True):
        parser.error(f"install vpk {VPK_VERSION} to match the Rust SDK")
    system = platform.system()
    feed = channel(system, platform.machine())
    source = ROOT / "target/release"
    stage = args.stage.resolve()
    stage.mkdir(parents=True, exist_ok=False)
    exe = "findanything.exe" if system == "Windows" else "findanything"
    icon = ROOT / "apps/native/icons" / ("icon.ico" if system == "Windows" else "128x128.png")
    sign = []
    if system == "Darwin":
        exe = "FindAnythingNative"
        bundle = stage / "Find Anything.app"
        content = bundle / "Contents"
        (content / "MacOS").mkdir(parents=True)
        (content / "Resources").mkdir()
        shutil.copy2(ROOT / "apps/native/macos/.build/release" / exe, content / "MacOS" / exe)
        shutil.copy2(ROOT / "apps/native/icons/icon.icns", content / "Resources/icon.icns")
        with (content / "Info.plist").open("wb") as file:
            plistlib.dump({"CFBundleIdentifier": "ing.findanyth.desktop", "CFBundleName": "Find Anything",
                          "CFBundleExecutable": exe, "CFBundlePackageType": "APPL", "CFBundleIconFile": "icon",
                          "CFBundleVersion": args.version, "CFBundleShortVersionString": args.version,
                          "LSMinimumSystemVersion": "13.0", "LSUIElement": True,
                          "NSHighResolutionCapable": True}, file)
        stage = bundle
        if args.signed:
            sign = ["--signAppIdentity", os.environ["APPLE_APP_IDENTITY"],
                    "--signInstallIdentity", os.environ["APPLE_INSTALLER_IDENTITY"],
                    "--notaryProfile", "findanything-release"]
        icon = ROOT / "apps/native/icons/icon.icns"
    else:
        shutil.copy2(source / exe, stage / exe)
        for pattern in ["*.dll", "*.so*", "*.dylib"]:
            for library in source.glob(pattern):
                shutil.copy2(library, stage / library.name)
        if system == "Windows" and args.signed:
            sign = ["--signParams", f'/sha1 {os.environ["WINDOWS_CERT_THUMBPRINT"]} /fd SHA256 /tr https://timestamp.digicert.com /td SHA256']
    # Distinct IDs also keep installer/archive asset names unique when all four
    # feeds are uploaded into the same GitHub release.
    subprocess.run(["vpk", "pack", "--packId", f"FindAnything-{feed.removeprefix('preview-')}", "--packTitle", "Find Anything",
                    "--packVersion", args.version, "--packDir", str(stage), "--mainExe", exe,
                    "--channel", feed, "--icon", str(icon), "--outputDir", str(args.output.resolve()), *sign], check=True)


if __name__ == "__main__":
    main()
