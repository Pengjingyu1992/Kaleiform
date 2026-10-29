"""Publication policy tests use metadata and synthetic files; no GitHub writes."""
import copy
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('desktop_publication', Path(__file__).with_name('publish-desktop.py'))
publish = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publish)


class PublicationBoundaries(unittest.TestCase):
    def setUp(self):
        self.request = {'source_sha': 'a' * 40, 'version': '1.2.3-preview.1',
                        'checks': {name: i + 1 for i, name in enumerate(sorted(publish.CHECKS))}}
        self.run = {'head_sha': self.request['source_sha'], 'head_branch': 'main', 'event': 'push',
                    'name': 'CI', 'repository': {'full_name': publish.REPOSITORY},
                    'head_repository': {'full_name': publish.REPOSITORY}, 'status': 'completed', 'conclusion': 'success'}

    def test_approved_request(self):
        publish.validate_request(self.request, publish.REPOSITORY)
        self.assertTrue(publish.run_ready(self.run, self.request, 'CI'))

    def test_pending_run_cannot_publish(self):
        self.run.update(status='in_progress', conclusion=None)
        self.assertFalse(publish.run_ready(self.run, self.request, 'CI'))

    def test_failed_run_cannot_publish(self):
        self.run['conclusion'] = 'failure'
        with self.assertRaises(ValueError):
            publish.run_ready(self.run, self.request, 'CI')

    def test_other_source_branch_event_or_repository_cannot_publish(self):
        cases = [('head_sha', 'b' * 40), ('head_branch', 'other'), ('event', 'pull_request'),
                 ('head_repository', {'full_name': 'someone/fork'}), ('name', 'another workflow')]
        for key, value in cases:
            with self.subTest(key=key):
                run = copy.deepcopy(self.run)
                run[key] = value
                with self.assertRaises(ValueError):
                    publish.run_ready(run, self.request, 'CI')

    def test_all_required_workflows_must_be_pinned(self):
        del self.request['checks']['CI']
        with self.assertRaises(ValueError):
            publish.validate_request(self.request, publish.REPOSITORY)

    def test_wrong_repository_and_unsafe_version_are_rejected(self):
        with self.assertRaises(ValueError):
            publish.validate_request(self.request, 'someone/fork')
        self.request['version'] = '../macos'
        with self.assertRaises(ValueError):
            publish.validate_request(self.request, publish.REPOSITORY)

    def test_desktop_release_keeps_mac_tag_and_latest_separate(self):
        payload = publish.release_payload(self.request, 'Synthetic notes')
        self.assertEqual(payload['tag_name'], 'desktop-v1.2.3-preview.1')
        self.assertEqual(payload['target_commitish'], self.request['source_sha'])
        self.assertTrue(payload['prerelease'])
        self.assertEqual(payload['make_latest'], 'false')

    def test_existing_asset_can_only_be_reused_byte_for_byte(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'synthetic-windows.msi'
            path.write_bytes(b'synthetic payload, never executed')
            digest = publish.LINUX.sha256(path)
            asset = {'id': 2, 'name': path.name, 'size': path.stat().st_size, 'digest': 'sha256:' + digest, 'state': 'uploaded'}
            self.assertEqual(publish.LINUX.pending_assets({'assets': [asset]}, path.parent, {path.name: digest}), [])
            asset['digest'] = 'sha256:' + '0' * 64
            with self.assertRaises(ValueError):
                publish.LINUX.pending_assets({'assets': [asset]}, path.parent, {path.name: digest})

    def test_mac_asset_removal_or_change_is_detected(self):
        before = {'assets': [{'id': 1, 'name': 'original-mac.dmg', 'size': 10, 'digest': 'sha256:' + 'a' * 64}]}
        publish.LINUX.verify_retained(before, copy.deepcopy(before))
        for after in ({'assets': []}, {'assets': [dict(before['assets'][0], size=11)]}):
            with self.assertRaises(ValueError):
                publish.LINUX.verify_retained(before, after)

    def test_windows_release_manifest_binds_both_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            base = 'vectorcraft-' + self.request['version'] + '-windows-x64'
            msi = root / (base + '.msi')
            msi.write_bytes(b'synthetic MSI placeholder, never executed')
            archive = root / (base + '-portable.zip')
            with zipfile.ZipFile(archive, 'w') as stream:
                for name in publish.WINDOWS.REQUIRED_DOCS | {'docs/licenses/OFL-fixture.txt'}:
                    stream.writestr('Kaleiform/' + name, b'Synthetic licence fixture')
                stream.writestr('Kaleiform/vectorcraft.exe', b'MZsynthetic desktop')
                stream.writestr('Kaleiform/vectorcraft-cli.exe', b'MZsynthetic CLI')
            (root / (base + '.sha256')).write_text(''.join(hashlib.sha256(path.read_bytes()).hexdigest() + '  ' + path.name + '\n' for path in (msi, archive)))
            self.assertEqual(len(publish.audit_windows(root, self.request['version'])), 3)
            msi.write_bytes(b'changed')
            with self.assertRaises(ValueError):
                publish.audit_windows(root, self.request['version'])


if __name__ == '__main__':
    unittest.main()
