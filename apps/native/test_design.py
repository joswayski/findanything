"""Checks the shared source and drift guard, not just today's generated files."""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

DESIGN = Path(__file__).resolve().parent / "design"


class GraphiteTests(unittest.TestCase):
    def test_checked_in_outputs_match_source(self):
        subprocess.run([sys.executable, str(DESIGN / "generate.py"), "--check"], check=True)

    def test_check_rejects_stale_or_missing_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            design = root / "design"
            design.mkdir()
            (root / "macos/Sources/FindAnythingNative").mkdir(parents=True)
            for name in ("generate.py", "tokens.json"):
                (design / name).write_bytes((DESIGN / name).read_bytes())
            command = [sys.executable, str(design / "generate.py")]
            subprocess.run(command, check=True, capture_output=True)
            for name in ("graphite.rs", "colors.css", "../macos/Sources/FindAnythingNative/Graphite.swift"):
                path = design / name
                original = path.read_text()
                path.write_text(original + "stale\n")
                self.assertNotEqual(subprocess.run(command + ["--check"], capture_output=True).returncode, 0)
                path.unlink()
                self.assertNotEqual(subprocess.run(command + ["--check"], capture_output=True).returncode, 0)
                path.write_text(original)
            self.assertEqual(subprocess.run(command + ["--check"], capture_output=True).returncode, 0)


if __name__ == "__main__":
    unittest.main()
