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

    def test_rejects_instrumented_capture(self):
        raw = copy.deepcopy(self.raw)
        raw['result']['instrumented'] = True
        with self.assertRaises(ValueError):
            runner.validate(raw, 100)

    def test_rejects_known_diagnostic_artifacts(self):
        runner.validate_timing_build({'buildFeatures': []})
        for features in [['cpu-profile'], ['jit-profile', 'exit-stats'], ['jit-tests']]:
            record = {'artifactBuild': {'buildFeatures': features}}
            with self.subTest(features=features), self.assertRaises(ValueError):
                runner.validate_timing_build(record)


class PocketTankValidationTests(unittest.TestCase):
    def setUp(self):
        self.raw = {'result': {'workload': 'pocket-tank', 'passed': True, 'status': 'completed', 'stopCode': 0,
                              'verdict': None, 'checks': [{'name': 'model_decisions', 'count': 12, 'min': 10}],
                              'instructions': 100, 'jit': {'failed': 0, 'compiled': 1},
                              'provenance': {'sha256': {'asset/wasm': 'hash'}}}}

    def test_accepts_completed_run_without_a_verdict(self):
        self.assertEqual(runner.validate(self.raw, 100, 'pocket-tank')['instructions'], 100)

    def test_rejects_other_workload_and_failed_checks(self):
        with self.assertRaisesRegex(ValueError, 'workload'):
            runner.validate(self.raw, 100, 'tinydraw')
        for checks in ([], [{'name': 'model_decisions', 'count': 3, 'min': 10}]):
            with self.subTest(checks=checks):
                raw = copy.deepcopy(self.raw)
                raw['result']['checks'] = checks
                with self.assertRaisesRegex(ValueError, 'checks failed'):
                    runner.validate(raw, 100, 'pocket-tank')

    def test_workloads_name_their_assets(self):
        self.assertEqual(runner.WORKLOADS['tinydraw']['assets'], ['rom', 'bootloader', 'ptable', 'app', 'elf'])
        self.assertIn('model', runner.WORKLOADS['pocket-tank']['assets'])


if __name__ == '__main__':
    unittest.main()
