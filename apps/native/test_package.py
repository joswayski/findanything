import os
import plistlib
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import package


class PackageTests(unittest.TestCase):
    def test_channels_do_not_mix_operating_systems_or_architectures(self):
        self.assertEqual(
            [
                package.channel("Darwin", "arm64"),
                package.channel("Darwin", "x86_64"),
                package.channel("Windows", "AMD64"),
                package.channel("Linux", "x86_64"),
            ],
            ["preview-osx-arm64", "preview-osx-x64", "preview-win-x64", "preview-linux-x64"],
        )

    def test_staging_and_signing_commands_for_each_host(self):
        for system, arch, executable, expected_channel in [
            ("Darwin", "arm64", "FindAnythingNative", "preview-osx-arm64"),
            ("Windows", "AMD64", "findanything.exe", "preview-win-x64"),
            ("Linux", "x86_64", "findanything", "preview-linux-x64"),
        ]:
            with self.subTest(system=system), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                native = root / "apps/native"
                source = (
                    native / "macos/.build/release"
                    if system == "Darwin"
                    else root / "target/release"
                )
                source.mkdir(parents=True)
                (source / executable).write_bytes(b"fixture binary")
                (native / "icons").mkdir(parents=True)
                (native / "icons/icon.icns").write_bytes(b"fixture icon")
                (native / "design/lucide").mkdir(parents=True)
                (native / "design/lucide/LICENSE").write_bytes(b"fixture license")
                (native / "design/fonts").mkdir(parents=True)
                (native / "design/fonts/LICENSE.txt").write_bytes(b"font license")
                (native / "design/fonts/Inter-Regular.ttf").write_bytes(b"font bytes")
                argv = [
                    "package.py",
                    "--signed",
                    "--version",
                    "0.2.17",
                    "--stage",
                    str(root / "stage"),
                    "--output",
                    str(root / "out"),
                ]
                with (
                    patch.object(package, "ROOT", root),
                    patch("sys.argv", argv),
                    patch("platform.system", return_value=system),
                    patch("platform.machine", return_value=arch),
                    patch(
                        "subprocess.check_output",
                        return_value="Description:\n  \x1b[1mVelopack CLI\n  1.2.158, for distributing applications.\x1b[0m",
                    ) as version,
                    patch("subprocess.run") as run,
                    patch.dict(
                        os.environ,
                        {
                            "APPLE_APP_IDENTITY": "test app",
                            "APPLE_INSTALLER_IDENTITY": "test installer",
                            "WINDOWS_CERT_THUMBPRINT": "test-thumbprint",
                        },
                    ),
                ):
                    package.main()
                    version.assert_called_once_with(["vpk", "--help", "--legacyConsole"], text=True)
                    command = run.call_args.args[0]
                    self.assertEqual(command[command.index("--channel") + 1], expected_channel)
                    self.assertEqual(
                        command[command.index("--packId") + 1],
                        "FindAnything-" + expected_channel.removeprefix("preview-"),
                    )
                    self.assertEqual(command[command.index("--mainExe") + 1], executable)
                    self.assertTrue(run.call_args.kwargs["check"])
                    if system == "Darwin":
                        info = plistlib.loads(
                            (root / "stage/Find Anything.app/Contents/Info.plist").read_bytes()
                        )
                        self.assertEqual(info["CFBundleIdentifier"], "ing.findanyth.desktop")
                        self.assertEqual(info["CFBundleVersion"], "0.2.17")
                        self.assertTrue(info["LSUIElement"])
                        self.assertIn("--notaryProfile", command)
                    elif system == "Windows":
                        self.assertIn("--signParams", command)
                    notices = (
                        root / "stage/Find Anything.app/Contents/Resources"
                        if system == "Darwin"
                        else root / "stage"
                    )
                    self.assertEqual(
                        (notices / "Lucide-LICENSE.txt").read_bytes(), b"fixture license"
                    )
                    self.assertEqual((notices / "Inter-LICENSE.txt").read_bytes(), b"font license")
                    if system == "Darwin":
                        self.assertEqual(
                            (notices / "fonts/Inter-Regular.ttf").read_bytes(), b"font bytes"
                        )
                    # Never overwrite another staging tree, even on rerun.
                    with self.assertRaises(FileExistsError):
                        package.main()


if __name__ == "__main__":
    unittest.main()
