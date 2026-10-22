from __future__ import annotations

import importlib.util
import subprocess
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "skills" / "cli-coingecko" / "scripts" / "build-url.py"
SPEC = importlib.util.spec_from_file_location("build_url", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class BuildUrlTests(unittest.TestCase):
    def test_builds_window_across_year_boundary(self) -> None:
        self.assertEqual(
            MODULE.build_url("bitcoin", "2025-01-01"),
            "https://coingecko.com/en/coins/bitcoin/historical_data?start=2024-12-31&end=2025-01-02",
        )

    def test_rejects_non_slug(self) -> None:
        with self.assertRaises(ValueError):
            MODULE.build_url("BTC/USD", "2025-01-01")

    def test_cli_rejects_dates_without_neighbors_cleanly(self) -> None:
        for raw_date in ("0001-01-01", "9999-12-31"):
            with self.subTest(raw_date=raw_date):
                result = subprocess.run(
                    [sys.executable, str(SCRIPT), "bitcoin", raw_date], capture_output=True, text=True, check=False
                )
                self.assertEqual(result.returncode, 2)
                self.assertNotIn("Traceback", result.stderr)


if __name__ == "__main__":
    unittest.main()
