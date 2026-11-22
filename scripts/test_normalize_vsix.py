import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('normalize_vsix', Path(__file__).with_name('normalize-vsix.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class NormalizeTests(unittest.TestCase):
    def test_same_content_is_byte_identical_despite_order_times_comments(self):
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory)/name for name in ('a.vsix', 'b.vsix')]
            for index, p in enumerate(paths):
                with zipfile.ZipFile(p, 'w') as archive:
                    archive.comment = str(index).encode()
                    for name in (['extension/bin/linux-x64/fsharp-navigator', 'extension/package.json'][::(-1 if index else 1)]):
                        entry = zipfile.ZipInfo(name, (2020 + index, 1, 1, 0, 0, 0))
                        entry.external_attr = (0o100755 if '/bin/' in name else 0o100644) << 16
                        archive.writestr(entry, name.encode())
                module.normalize(p, 1700000000)
            self.assertEqual(paths[0].read_bytes(), paths[1].read_bytes())
            before = paths[0].read_bytes()
            module.normalize(paths[0], 1700000000)
            self.assertEqual(paths[0].read_bytes(), before)
            with zipfile.ZipFile(paths[0]) as archive:
                self.assertTrue((archive.getinfo('extension/bin/linux-x64/fsharp-navigator').external_attr >> 16) & 0o111)
                self.assertEqual(archive.read('extension/package.json'), b'extension/package.json')
    def test_duplicate_entries_leave_original_unchanged(self):
        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory)/'a.vsix'
            with zipfile.ZipFile(p, 'w') as archive:
                archive.writestr('duplicate', b'a')
                with self.assertWarns(UserWarning): archive.writestr('duplicate', b'b')
            before = p.read_bytes()
            with self.assertRaises(ValueError): module.normalize(p, 1700000000)
            self.assertEqual(p.read_bytes(), before)
    def test_bad_epoch_leaves_original_unchanged(self):
        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory)/'a.vsix'; p.write_bytes(b'unchanged')
            with self.assertRaises(ValueError): module.normalize(p, 0)
            self.assertEqual(p.read_bytes(), b'unchanged')

if __name__ == '__main__': unittest.main()
