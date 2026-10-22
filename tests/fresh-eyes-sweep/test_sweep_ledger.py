from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "skills" / "fresh-eyes-sweep" / "scripts" / "sweep-ledger.py"


def git(repo: Path, *args: str) -> None:
    subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True)


class SweepLedgerTests(unittest.TestCase):
    def helper(self, *args: str, check: bool = True):
        result = subprocess.run([sys.executable, str(SCRIPT), *args], text=True, capture_output=True)
        if check:
            self.assertEqual(result.returncode, 0, result.stderr)
        return result

    def test_scope_mapping_statuses_preexisting_and_exact_progress(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            repo = root / "repo"
            repo.mkdir()
            git(repo, "init", "-q", "-b", "main")
            git(repo, "config", "user.name", "Test")
            git(repo, "config", "user.email", "test@example.com")
            (repo / "src").mkdir()
            (repo / "docs").mkdir()
            (repo / "src" / "a.py").write_text("a = 1\n")
            (repo / "src" / "b.py").write_text("b = 1\n")
            (repo / "docs" / "readme.md").write_text("docs\n")
            (repo / ".gitignore").write_text("ignored\n")
            git(repo, "add", ".")
            git(repo, "commit", "-qm", "initial")
            (repo / "src" / "a.py").write_text("a = 2\n")
            (repo / "src" / "new.py").write_text("new = 1\n")
            (repo / "ignored").write_text("ignored\n")
            ledger = root / "ledger.json"
            initialized = json.loads(self.helper("init", "--root", str(repo), "--ledger", str(ledger), "src").stdout)
            self.assertEqual(initialized["counts"]["mapped"], 3)
            payload = json.loads(ledger.read_text())
            by_path = {item["path"]: item for item in payload["files"]}
            self.assertTrue(by_path["src/a.py"]["preexistingStatus"])
            self.assertFalse(by_path["src/new.py"]["tracked"])
            self.assertNotIn("ignored", by_path)

            failed = self.helper("mark", "--ledger", str(ledger), "--status", "excluded", "--path", "src/a.py", check=False)
            self.assertNotEqual(failed.returncode, 0)
            self.assertEqual(json.loads(ledger.read_text())["revision"], 0)
            self.helper("mark", "--ledger", str(ledger), "--status", "inspected", "--path", "src/a.py")
            marked = json.loads(self.helper(
                "mark", "--ledger", str(ledger), "--status", "excluded", "--reason", "agent classified generated",
                "--path", "src/b.py",
            ).stdout)
            self.assertEqual(marked["counts"]["accounted"], 2)
            self.assertEqual(marked["progress"]["bar"], "███████░░░")
            pending = json.loads(self.helper("pending", "--ledger", str(ledger), "--limit", "1").stdout)
            self.assertEqual(pending["paths"], ["src/new.py"])
            final = json.loads(self.helper("mark", "--ledger", str(ledger), "--status", "fixed", "--path", "src/new.py").stdout)
            self.assertTrue(final["complete"])
            self.assertEqual(final["counts"]["fixed"], 1)

    def test_untracked_paths_match_ls_files_others(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            repo = root / "repo"
            repo.mkdir()
            git(repo, "init", "-q", "-b", "main")
            git(repo, "config", "user.name", "Test")
            git(repo, "config", "user.email", "test@example.com")
            (repo / ".gitignore").write_text("ignored/\n*.log\n!keep.log\n")
            (repo / "moved.py").write_text("v = 1\n")
            git(repo, "add", ".gitignore", "moved.py")
            git(repo, "commit", "-qm", "initial")
            (repo / "ignored").mkdir()
            (repo / "ignored" / "f.txt").write_text("x\n")
            (repo / "x.log").write_text("x\n")
            (repo / "keep.log").write_text("x\n")
            (repo / "deep" / "er").mkdir(parents=True)
            (repo / "deep" / "er" / "u.txt").write_text("x\n")
            (repo / "with space.txt").write_text("x\n")
            (repo / "caf\u00e9.txt").write_text("x\n")
            git(repo, "mv", "moved.py", "renamed.py")
            (repo / "moved.py").write_text("v = 2\n")
            ledger = root / "ledger.json"
            self.helper("init", "--root", str(repo), "--ledger", str(ledger))
            recorded = {item["path"] for item in json.loads(ledger.read_text())["files"]}
            listed = subprocess.run(
                ["git", "-C", str(repo), "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
                capture_output=True, check=True,
            ).stdout
            expected = {value.decode() for value in listed.split(b"\0") if value}
            self.assertEqual(recorded, expected)
            self.assertIn("moved.py", recorded)
            self.assertIn("deep/er/u.txt", recorded)
            self.assertIn("keep.log", recorded)
            self.assertNotIn("x.log", recorded)
            self.assertNotIn("ignored/f.txt", recorded)

    def test_unknown_scope_and_path_fail_without_partial_change(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            repo = root / "repo"
            repo.mkdir()
            git(repo, "init", "-q")
            git(repo, "config", "user.name", "Test")
            git(repo, "config", "user.email", "test@example.com")
            (repo / "a").write_text("a")
            git(repo, "add", "a")
            git(repo, "commit", "-qm", "initial")
            ledger = root / "ledger.json"
            missing_scope = self.helper("init", "--root", str(repo), "--ledger", str(ledger), "typo", check=False)
            self.assertEqual(missing_scope.returncode, 64)
            self.assertIn("typo", missing_scope.stderr)
            self.assertFalse(ledger.exists())
            self.helper("init", "--root", str(repo), "--ledger", str(ledger))
            before = ledger.read_bytes()
            result = self.helper(
                "mark", "--ledger", str(ledger), "--status", "reported", "--path", "a", "--path", "missing",
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(ledger.read_bytes(), before)

    def test_concurrent_marks_from_parallel_subagents_lose_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            repo = root / "repo"
            repo.mkdir()
            git(repo, "init", "-q", "-b", "main")
            git(repo, "config", "user.name", "Test")
            git(repo, "config", "user.email", "test@example.com")
            names = [f"file{index}.py" for index in range(12)]
            for name in names:
                (repo / name).write_text("x = 1\n")
            git(repo, "add", ".")
            git(repo, "commit", "-qm", "initial")
            ledger = root / "ledger.json"
            self.helper("init", "--root", str(repo), "--ledger", str(ledger))
            workers = [
                subprocess.Popen(
                    [sys.executable, str(SCRIPT), "mark", "--ledger", str(ledger), "--status", "inspected", "--path", name],
                    text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                )
                for name in names
            ]
            for worker in workers:
                _, stderr = worker.communicate()
                self.assertEqual(worker.returncode, 0, stderr)
            final = json.loads(self.helper("summary", "--ledger", str(ledger)).stdout)
            self.assertTrue(final["complete"])
            self.assertEqual(final["revision"], len(names))


if __name__ == "__main__":
    unittest.main()
