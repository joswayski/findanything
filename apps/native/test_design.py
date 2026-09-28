"""Checks the shared source and drift guard, not just today's generated files."""
from pathlib import Path
import math
import shutil
import subprocess
import sys
import tempfile
import unittest

from design import generate_icons

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


class LucideTests(unittest.TestCase):
    def test_relative_coordinates_subpaths_and_close(self):
        self.assertEqual(generate_icons.path_points("M3 7h5v-2l-1 4z m10 2 3 4H20V6"), [
            [(3, 7), (8, 7), (8, 5), (7, 9), (3, 7)],
            [(13, 9), (16, 13), (20, 13), (20, 6)],
        ])

    def test_circular_arc_direction_and_radius(self):
        clockwise = generate_icons.path_points("M2 0a2 2 0 0 1 -2 2")[0]
        counter = generate_icons.path_points("M2 0A2 2 0 0 0 0 2")[0]
        self.assertEqual(clockwise[-1], (0, 2))
        self.assertEqual(counter[-1], (0, 2))
        midpoint = clockwise[len(clockwise) // 2]
        self.assertAlmostEqual(midpoint[0], math.sqrt(2))
        self.assertAlmostEqual(midpoint[1], math.sqrt(2))
        midpoint = counter[len(counter) // 2]
        self.assertAlmostEqual(midpoint[0], 2 - math.sqrt(2))
        self.assertAlmostEqual(midpoint[1], 2 - math.sqrt(2))
        for x, y in clockwise:
            self.assertAlmostEqual(x*x + y*y, 4)

    def test_unsupported_paths_fail(self):
        for data in ("M0 0Q1 2 3 4", "M0 0A2 3 0 0 1 4 5", "M0 0L2"):
            with self.subTest(data=data), self.assertRaises(ValueError):
                generate_icons.path_points(data)

    def test_generated_outputs_and_drift_guard(self):
        subprocess.run([sys.executable, str(DESIGN / "generate_icons.py"), "--check"], check=True)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            design = root / "design"
            shutil.copytree(DESIGN / "lucide", design / "lucide")
            shutil.copy2(DESIGN / "generate_icons.py", design)
            (root / "macos/Sources/FindAnythingNative").mkdir(parents=True)
            command = [sys.executable, str(design / "generate_icons.py")]
            subprocess.run(command, check=True, capture_output=True)
            for name in ("lucide.rs", "../macos/Sources/FindAnythingNative/Lucide.swift"):
                path = design / name
                original = path.read_text()
                path.write_text(original + "stale\n")
                self.assertNotEqual(subprocess.run(command + ["--check"], capture_output=True).returncode, 0)
                path.unlink()
                self.assertNotEqual(subprocess.run(command + ["--check"], capture_output=True).returncode, 0)
                path.write_text(original)
            subprocess.run(command + ["--check"], check=True, capture_output=True)


if __name__ == "__main__":
    unittest.main()
