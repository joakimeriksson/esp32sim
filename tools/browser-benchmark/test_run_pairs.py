import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('run_pairs', Path(__file__).with_name('run-pairs.py'))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class ValidationTests(unittest.TestCase):
    def setUp(self):
        self.raw = {'result': {'passed': True, 'status': 'completed', 'stopCode': 0,
                              'verdict': ' '.join([runner.comparator.SCHEMA['marker'], *[f'{key}=1' for key in runner.comparator.SCHEMA['gates']], 'ssaa_receipt=yellow']),
                              'instructions': 100, 'jit': {'failed': 0, 'compiled': 1},
                              'provenance': {'sha256': {'asset/wasm': 'hash'}}}}

    def test_accepts_completed_run(self):
        self.assertEqual(runner.validate(self.raw, 100)['instructions'], 100)

    def test_rejects_invalid_runs(self):
        mutations = [('passed', False), ('status', 'stopped'), ('stopCode', 1),
                     ('instructions', 101), ('verdict', 'incomplete'), ('jit', {'failed': 1, 'compiled': 1}),
                     ('jit', {'failed': 0, 'compiled': 0}), ('provenance', {})]
        for key, value in mutations:
            with self.subTest(key=key, value=value):
                raw = copy.deepcopy(self.raw)
                raw['result'][key] = value
                with self.assertRaises(ValueError):
                    runner.validate(raw, 100)


if __name__ == '__main__':
    unittest.main()
