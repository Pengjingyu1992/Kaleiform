"""The automatic packaging tool must be verified before it becomes executable."""

from pathlib import Path
import subprocess
import tempfile
import unittest


class AppImageToolBoundary(unittest.TestCase):
    def test_corrupt_cache_is_rejected_without_execution_or_download(self):
        with tempfile.TemporaryDirectory() as directory:
            tool = Path(directory) / "tool.AppImage"
            tool.write_text("#!/bin/sh\nexit 77\n")
            tool.chmod(0o600)
            result = subprocess.run(
                [
                    "bash",
                    "-c",
                    'source "$1"; fetch_appimagetool "$2" x86_64',
                    "test",
                    str(Path(__file__).with_name("appimagetool.sh")),
                    str(tool),
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("FAILED", result.stdout)
            self.assertEqual(tool.stat().st_mode & 0o111, 0)

    def test_download_mismatch_leaves_no_tool_or_partial_file(self):
        with tempfile.TemporaryDirectory() as directory:
            tool = Path(directory) / "tool.AppImage"
            result = subprocess.run(
                [
                    "bash",
                    "-c",
                    """
source "$1"
curl() {
  while [ "$#" -gt 0 ]; do
    if [ "$1" = --output ]; then printf 'synthetic invalid download' > "$2"; return; fi
    shift
  done
  return 2
}
fetch_appimagetool "$2" x86_64
""",
                    "test",
                    str(Path(__file__).with_name("appimagetool.sh")),
                    str(tool),
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("FAILED", result.stdout)
            self.assertEqual(list(Path(directory).iterdir()), [])


if __name__ == "__main__":
    unittest.main()
