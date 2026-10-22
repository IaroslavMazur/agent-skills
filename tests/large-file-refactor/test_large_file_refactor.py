from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "skills" / "large-file-refactor" / "scripts" / "large-file-refactor.py"


class LargeFileRefactorTests(unittest.TestCase):
    def test_ancestor_directory_names_do_not_ignore_or_reclassify_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            project = Path(directory) / "build" / "tests" / "project"
            for relative, lines in (("src/big.py", 1100), ("vendor/big.py", 1100), ("tests/big.py", 1500)):
                path = project / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("x = 1\n" * lines)
            # The second PATH hides tokei and rg so the portable fallback runs.
            for search_path in (os.environ.get("PATH", ""), os.pathsep.join(("/usr/bin", "/bin"))):
                with self.subTest(path=search_path):
                    result = subprocess.run(
                        [sys.executable, str(SCRIPT), str(project)],
                        text=True,
                        capture_output=True,
                        env={**os.environ, "PATH": search_path},
                        check=False,
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn("`src/big.py` (1100 LOC)", result.stdout)
                    self.assertNotIn("vendor/big.py", result.stdout)
                    self.assertNotIn("tests/big.py", result.stdout)


if __name__ == "__main__":
    unittest.main()
