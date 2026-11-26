import datetime as dt
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('age', Path(__file__).with_name('dependency-age.py'))
age = importlib.util.module_from_spec(spec)
spec.loader.exec_module(age)


class AgeTests(unittest.TestCase):
    def test_exact_boundary_and_future_release(self):
        now = dt.datetime(2026, 11, 25, tzinfo=dt.timezone.utc)
        age.validate(('cargo', 'fixture', '1'), '2026-11-23T00:00:00Z', True, now)
        for stamp in ['2026-11-23T00:00:01Z', '2026-11-26T00:00:00Z']:
            with self.assertRaisesRegex(ValueError, '48 hours'):
                age.validate(('cargo', 'fixture', '1'), stamp, True, now)

    def test_old_yanked_or_unlisted_release_is_rejected(self):
        with self.assertRaisesRegex(ValueError, 'yanked or unlisted'):
            age.validate(('nuget', 'fixture', '1'), '2020-01-01T00:00:00Z', False, dt.datetime.now(dt.timezone.utc))


if __name__ == '__main__':
    unittest.main()
