"""Check the distribution boundary with synthetic payloads, never user documents."""
from pathlib import Path
import tempfile
import unittest
import warnings
import zipfile

from privacy import check


class WindowsPayloadChecks(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='kaleiform-privacy-')
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / 'payload.zip'
        self.files = {
            'vectorcraft.exe': b'MZsynthetic desktop',
            'vectorcraft-cli.exe': b'MZsynthetic CLI',
            'docs/NOTICE': b'Synthetic notice fixture',
            'docs/UPSTREAM.md': b'Synthetic provenance fixture',
            'docs/ASSETS.md': b'Synthetic asset fixture',
            'docs/LICENSE-MIT': b'Synthetic licence fixture',
            'docs/LICENSE-APACHE': b'Synthetic licence fixture',
            'docs/licenses/LICENSE-linesweeper-MIT': b'Synthetic licence fixture',
            'docs/licenses/LICENSE-linesweeper-APACHE': b'Synthetic licence fixture',
            'docs/licenses/LICENSE-wgpu-hal-MIT': b'Synthetic licence fixture',
            'docs/licenses/LICENSE-wgpu-hal-APACHE': b'Synthetic licence fixture',
            'docs/licenses/LICENSE-lucide.txt': b'Synthetic licence fixture',
            'docs/licenses/OFL-fixture.txt': b'Synthetic licence fixture',
        }

    def archive(self, extras=()):
        with zipfile.ZipFile(self.path, 'w') as archive:
            for name, data in self.files.items():
                archive.writestr('Kaleiform/' + name, data)
            for name, data in extras:
                archive.writestr(name, data)

    def rejected(self, extras=()):
        self.archive(extras)
        with self.assertRaises(ValueError):
            check(self.path)

    def test_valid_payload(self):
        self.archive()
        check(self.path)

    def test_missing_notice_is_rejected(self):
        del self.files['docs/NOTICE']
        self.rejected()

    def test_missing_font_licence_is_rejected(self):
        del self.files['docs/licenses/OFL-fixture.txt']
        self.rejected()

    def test_private_path_is_rejected(self):
        self.files['vectorcraft.exe'] += br'C:\Users\SamplePerson\Documents\drawing'
        self.rejected()

    def test_traversal_and_drive_paths_are_rejected(self):
        for name in ('Kaleiform/../private.txt', 'C:/private.txt'):
            with self.subTest(name=name):
                self.rejected(((name, b'synthetic'),))

    def test_extra_executable_is_rejected(self):
        self.rejected((('Kaleiform/docs/vectorcraft.exe', b'MZsynthetic'),))

    def test_debug_and_local_config_are_rejected(self):
        for name in ('Kaleiform/app.pdb', 'Kaleiform/.GIT/config'):
            with self.subTest(name=name):
                self.rejected(((name, b'synthetic'),))

    def test_case_insensitive_duplicate_is_rejected(self):
        with warnings.catch_warnings():
            warnings.simplefilter('ignore', UserWarning)
            self.rejected((('Kaleiform/VECTORCRAFT.EXE', b'MZsynthetic'),))


if __name__ == '__main__':
    unittest.main()
