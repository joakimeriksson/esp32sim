import importlib.util
import json
import pathlib
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('compare_runs', pathlib.Path(__file__).with_name('compare-runs.py'))
compare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compare)
VERDICT = ' '.join([compare.SCHEMA['marker'],
                    *(f'{key}=1' for key in compare.SCHEMA['gates']), 'ssaa_receipt=yellow'])


class VerdictTests(unittest.TestCase):
    def test_comparison_retains_and_matches_firmware_provenance(self):
        run = {'instructions': 100, 'consoleSha256': 'console', 'verdict': VERDICT,
               'browser': '1', 'v8': '1', 'wallSeconds': 10,
               'intervalSeconds': {name: 1 for name, _ in compare.MILESTONES},
               'provenance': {'sha256': {f'asset/{name}': name for name in
                                       ('rom', 'bootloader', 'ptable', 'app', 'elf', 'wasm')}}}
        candidate = json.loads(json.dumps(run))
        candidate['provenance']['sha256']['asset/wasm'] = 'candidate'
        self.assertEqual(compare.comparison([run], [candidate])['candidate'], [candidate])
        mixed = json.loads(json.dumps(run))
        mixed['provenance']['sha256']['asset/wasm'] = 'mixed'
        with self.assertRaisesRegex(ValueError, 'mixed build'):
            compare.comparison([run, mixed], [candidate, candidate])
        candidate['provenance']['sha256']['asset/app'] = 'different-firmware'
        with self.assertRaisesRegex(ValueError, 'asset/app'):
            compare.comparison([run], [candidate])
        candidate['provenance'] = None
        with self.assertRaisesRegex(ValueError, 'provenance'):
            compare.comparison([run], [candidate])

    def test_schema(self):
        self.assertEqual(len(compare.SCHEMA['gates']), 36)
        compare.validate_verdict(VERDICT)
        for line in (None, '', compare.SCHEMA['marker'],
                     VERDICT.replace('stress=1 ', ''), VERDICT + ' stress=1',
                     VERDICT + ' surprise=1', VERDICT.replace('stress=1', 'stress=0'),
                     VERDICT.replace('stress=1', 'stress=true'),
                     VERDICT.replace('stress=1', 'stress=01'),
                     VERDICT.replace('stress=1', 'stress=1=1'),
                     VERDICT.replace(' ssaa_receipt=yellow', ''),
                     VERDICT.replace('yellow', 'unknown'), VERDICT + '\n'):
            with self.subTest(line=line), self.assertRaises(ValueError):
                compare.validate_verdict(line)

    def test_read_run_checks_console_not_just_passed_flag(self):
        # Old captures without a schema annotation remain valid if their actual
        # verdict and serial evidence satisfy the current explicit schema.
        with tempfile.TemporaryDirectory(dir=pathlib.Path(__file__).parent) as tmp:
            directory = pathlib.Path(tmp)
            capture = {'captureMode': 'timing', 'version': {'User-Agent': 'HeadlessChrome/1', 'Browser': '1', 'V8-Version': '1'},
                       'result': {'stopCode': 0, 'provenance': {'sha256': {'asset/wasm': 'wasm'}}, 'passed': True, 'status': 'completed', 'verdict': VERDICT,
                                  'wallSeconds': 10, 'instructions': 100, 'jit': {'instructions': 50, 'failed': 0}}}
            events = [{'type': 'serial', 'data': (VERDICT if marker == compare.SCHEMA['marker'] else marker) + '\n',
                       'wallMs': (i + 1) * 1000} for i, (_, marker) in enumerate(compare.MILESTONES)]
            def write():
                (directory / 'result.json').write_text(json.dumps(capture))
                (directory / 'events.json').write_text(json.dumps(events))
            write()
            self.assertEqual(compare.read_run(directory)['verdict'], VERDICT)
            for mode in [None, 'cpu-profile', 'jit-profile']:
                capture['captureMode'] = mode
                write()
                with self.assertRaisesRegex(ValueError, 'timing capture mode'):
                    compare.read_run(directory)
            capture['captureMode'] = 'timing'
            capture['result']['instrumented'] = True
            write()
            with self.assertRaisesRegex(ValueError, 'diagnostic exports'):
                compare.read_run(directory)
            capture['result']['instrumented'] = False
            for failed in [None, 123]:
                capture['result']['jit']['failed'] = failed
                write()
                with self.assertRaisesRegex(ValueError, 'JIT status'):
                    compare.read_run(directory)
            capture['result']['jit']['failed'] = 0
            provenance = capture['result'].pop('provenance')
            write()
            with self.assertRaisesRegex(ValueError, 'provenance'):
                compare.read_run(directory)
            capture['result']['provenance'] = provenance
            for ending in [VERDICT, VERDICT + '\n' + VERDICT + '\n',
                           VERDICT.replace('stress=1', 'stress=0') + '\n']:
                events[-1]['data'] = ending
                write()
                with self.assertRaises(ValueError):
                    compare.read_run(directory)
            events[-1]['data'] = compare.SCHEMA['marker'] + '\n'
            capture['result']['verdict'] = compare.SCHEMA['marker']
            write()
            with self.assertRaises(ValueError):
                compare.read_run(directory)


if __name__ == '__main__':
    unittest.main()
