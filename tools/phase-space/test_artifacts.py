"""Evidence rules and template absence checks; no simulated benchmark results."""
import pathlib
import tempfile
import unittest
import zipfile
from xml.etree import ElementTree as E
import artifacts

class ArtifactRules(unittest.TestCase):
    def test_verdict_requires_distinct_repeats_and_one_clean_build(self):
        rows = [dict(repeat=i, quotable=True, clean_tree=True, commit='a',
                     executable_sha256='b', case='c', frame_p95_ms=19., age_p95_ms=40.)
                for i in (1, 2, 3)]
        self.assertEqual(artifacts.verdict(rows), 'accepted')
        rows[0]['frame_p95_ms'] = 21.
        self.assertEqual(artifacts.verdict(rows), 'unresolved')
        rows[1]['frame_p95_ms'] = rows[2]['frame_p95_ms'] = 21.
        self.assertEqual(artifacts.verdict(rows), 'targets not met')
        rows[0]['repeat'] = 2
        self.assertEqual(artifacts.verdict(rows), 'unresolved')
        rows[0]['repeat'] = 1
        rows[0]['commit'] = 'different'
        self.assertEqual(artifacts.verdict(rows), 'unresolved')
        rows[0]['commit'] = 'a'
        rows[0]['clean_tree'] = False
        self.assertEqual(artifacts.verdict(rows), 'unresolved')

    def test_empty_copy_has_no_sample_values_or_shared_formulas(self):
        source = artifacts.ROOT / 'artifact-template-analytics-dashboard/assets/reference.xlsx'
        if not source.exists():
            self.skipTest('template plugin is not installed')
        before = source.read_bytes()
        with tempfile.TemporaryDirectory() as td:
            out = pathlib.Path(td) / 'empty.xlsx'
            artifacts.workbook(source, out, {}, [], {})
            with zipfile.ZipFile(out) as z:
                for name in ('xl/worksheets/sheet1.xml', 'xl/worksheets/sheet3.xml'):
                    root = E.fromstring(z.read(name))
                    for f in root.iter(artifacts.tag(artifacts.S, 'f')):
                        self.assertNotEqual(f.get('t'), 'shared')
                    for v in root.iter(artifacts.tag(artifacts.S, 'v')):
                        self.assertNotIn(v.text, ('31500', '403700', '4480'))
                data = E.fromstring(z.read('xl/worksheets/sheet2.xml'))
                for c in data.iter(artifacts.tag(artifacts.S, 'c')):
                    ref = c.get('r')
                    if ref[0] in 'CDEFGHI' and 16 <= int(ref[1:]) <= 27:
                        self.assertIsNone(c.find(artifacts.tag(artifacts.S, 'v')))
        self.assertEqual(source.read_bytes(), before)

if __name__ == '__main__':
    unittest.main()
