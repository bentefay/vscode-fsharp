"""Scorer and pinned-input controls; no native/compiler processes or downloads."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
from unittest.mock import patch
from engine_corpus_gate import Blocked, inventory, safe_path, score, main, validate, validate_contract, report_set_errors, finalize_reports, location, validate_known_contract, lifecycle_controls, score_references, reference_variant, reference_forbidden, audit_reference_obligations, validate_reference_contract

GOLD = {'uri': 'file:///source/A.fs', 'range': {'start': {'line': 1, 'character': 4}, 'end': {'line': 1, 'character': 7}}}
OLD = {'uri': 'file:///source/A.fs', 'range': {'start': {'line': 0, 'character': 4}, 'end': {'line': 0, 'character': 7}}}


class ScorerTests(unittest.TestCase):
    def test_exact_requires_full_singleton(self):
        for locations in [[], [OLD], [GOLD, OLD], [GOLD, GOLD]]:
            self.assertFalse(score({'result': {'completeness': 'exact', 'locations': locations}}, GOLD, 'exact')['passed'])
        self.assertTrue(score({'result': {'completeness': 'exact', 'locations': [GOLD]}}, GOLD, 'exact')['passed'])

    def test_candidate_cannot_satisfy_exact(self):
        value = {'completeness': 'unavailable', 'locations': [], 'candidates': [OLD, GOLD], 'candidateCompleteness': 'incomplete'}
        self.assertFalse(score({'result': value}, GOLD, 'exact')['passed'])
        scored = score({'result': value}, GOLD, 'useful')
        self.assertTrue(scored['passed']); self.assertEqual(scored['knownRank'], 2)

    def test_miss_and_empty_exact_are_not_useful(self):
        for value in [{'completeness': 'unavailable', 'locations': []}, {'completeness': 'exact', 'locations': []}]:
            self.assertFalse(score({'result': value}, GOLD, 'useful')['passed'])

    def test_uncertainty_and_locations_are_strict(self):
        for value in [{'completeness': 'unavailable', 'locations': [GOLD]}, {'completeness': 'unavailable', 'candidates': [GOLD]}]:
            self.assertFalse(score({'result': value}, GOLD, 'useful')['passed'])

    def test_former_target_forbidden_in_all_tiers(self):
        for key in ['locations', 'candidates', 'likelyCandidates']:
            value = {'completeness': 'unavailable', 'locations': [], 'candidates': [GOLD], 'candidateCompleteness': 'incomplete'}
            value.setdefault(key, []).append(OLD)
            self.assertEqual(score({'result': value}, GOLD, 'useful', [OLD])['classification'], 'stale-target')

    def test_likely_must_belong_to_union(self):
        value = {'completeness': 'unavailable', 'candidates': [GOLD], 'likelyCandidates': [OLD], 'candidateCompleteness': 'incomplete'}
        self.assertFalse(score({'result': value}, GOLD, 'useful')['passed'])

    def test_origin_version_is_required_and_consistent(self):
        good = {'completeness': 'exact', 'locations': [GOLD], 'version': 2, 'documentVersion': 2}
        self.assertTrue(score({'result': good}, GOLD, 'exact', expected_version=2)['passed'])
        for changes in [{'version': 1}, {'documentVersion': 1}, {'version': None}]:
            self.assertFalse(score({'result': dict(good, **changes)}, GOLD, 'exact', expected_version=2)['passed'])

    def test_protocol_error_cannot_pass(self):
        self.assertFalse(score({'error': {'message': 'timeout'}}, GOLD, 'useful')['passed'])


class PinTests(unittest.TestCase):
    def test_inventory_detects_add_edit_remove(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); policy = {'excludedDirectories': ['.git'], 'suffixes': ['.fs'], 'names': []}
            source = root/'A.fs'; source.write_text('module A\n')
            original = inventory(root, policy)
            source.write_text('module B\n'); self.assertNotEqual(original, inventory(root, policy))
            source.write_text('module A\n'); (root/'B.fs').write_text('module B\n'); self.assertNotEqual(original, inventory(root, policy))
            (root/'B.fs').unlink(); self.assertEqual(original, inventory(root, policy)); source.unlink(); self.assertNotEqual(original, inventory(root, policy))

    def test_missing_and_escape_block(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with self.assertRaises(Blocked): inventory(root/'missing', {})
            with self.assertRaises(Blocked): safe_path(root, '../elsewhere.fs')

    def test_preflight_block_never_starts_native(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for extra in [[], ['--compiler-root', str(root)]]:
                output = root/'result.json'
                with patch('engine_corpus_gate.run_suite') as launch:
                    code = main(['--binary', '/not-a-binary', '--suite', 'compiler', '--output', str(output), *extra])
                    launch.assert_not_called()
                self.assertEqual(code, 2)
                self.assertEqual(json.loads(output.read_text())['status'], 'blocked')

    def test_changed_source_blocks_without_launch(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); source = root/'src/Compiler'; source.mkdir(parents=True)
            (source/'Other.fs').write_text('module Other\n')
            output = root/'result.json'
            with patch('engine_corpus_gate.run_suite') as launch:
                code = main(['--binary', '/not-a-binary', '--suite', 'compiler', '--compiler-root', str(root), '--output', str(output)])
                launch.assert_not_called()
            self.assertEqual(code, 2)
            self.assertIn('inventory differs', json.loads(output.read_text())['blocker'])

    def test_new_ancestor_input_is_not_silently_accepted(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); source = root/'src'; source.mkdir()
            project = source/'A.fsproj'; project.write_text('<Project/>')
            policy = {'excludedDirectories': [], 'suffixes': ['.fs', '.fsproj', '.props', '.targets'], 'names': ['global.json']}
            suite = {'id': 'unit', 'workspace': 'src', 'inventory': dict(policy, **inventory(source, policy)), 'ancestorInputs': {}, 'project': 'src/A.fsproj', 'projectSha256': hashlib.sha256(project.read_bytes()).hexdigest(), 'required': []}
            validate(root, suite)
            (root/'Directory.Build.props').write_text('<Project/>')
            with self.assertRaisesRegex(Blocked, 'enclosing input set'):
                validate(root, suite)

    def test_manifest_denominators_and_required_contracts(self):
        data = json.loads((Path(__file__).parent/'engine-corpus/manifest.json').read_text())
        self.assertEqual([s['id'] for s in data['suites']], ['grace', 'fscheck', 'compiler'])
        self.assertEqual([len(s['required']) for s in data['suites']], [2, 3, 2])
        self.assertEqual(data['suites'][2]['required'][0]['require'], 'useful')
        self.assertTrue(all(s['lifecycle']['lines'] == 8 for s in data['suites']))


class ObligationTests(unittest.TestCase):
    def setUp(self):
        self.manifest = json.loads((Path(__file__).parent/'engine-corpus/manifest.json').read_text())
        self.suite = copy.deepcopy(self.manifest['suites'][2])
        # Definition-auditor unit fixtures are intentionally isolated; main's
        # contract validator independently requires the real reference case.
        self.suite['references'] = []
        self.root = Path('/portable-source')

    def report(self):
        first = self.suite['required'][0]
        rows = []
        for epoch in ['fresh', 'restart']:
            for initial, initial_site in [('first-response', first), ('discovery-trigger', self.suite['discoveryTrigger'])]:
                q = initial_site['query']; origin = str(self.root/q['path'])
                rows.append({'epoch': epoch, 'method': 'document/open', 'params': {'path': origin, 'version': 1}, 'response': {'result': {}}})
                rows.append({'epoch': epoch, 'method': 'navigation/definition', 'phase': initial, 'site': initial_site['id'], 'params': {'path': origin, 'line': q['line'], 'character': q['character'], 'documentVersion': 1}, 'response': {'result': {'completeness': 'unavailable', 'locations': [], 'version': 1}}, 'scoring': {'passed': False}})
            rows.append({'epoch': epoch, 'method': 'workspace/stats', 'response': {'result': {'background': {'phase': 'ready', 'discoveryComplete': True, 'indexComplete': True}}}})
            phases = [('ready', site, None) for site in self.suite['required']]
            if epoch == 'fresh':
                for control in lifecycle_controls(self.suite):
                    site = next(s for s in self.suite['required'] if s['id'] == control['site'])
                    phases += [('dirty', site, control), ('closed-restored', site, control)]
            for phase, site, control in phases:
                shifted = copy.deepcopy(site); version = 2 if phase == 'dirty' else 1
                if phase == 'dirty':
                    shifted['query']['line'] += control['lines']
                    for point in ['start', 'end']: shifted['expected']['range'][point]['line'] += control['lines']
                path = str(self.root/site['query']['path'])
                rows.append({'epoch': epoch, 'method': 'document/open', 'params': {'path': path, 'version': version}, 'response': {'result': {}}})
                q = shifted['query']; result = {'completeness': 'exact', 'locations': [location(self.root, shifted['expected'])], 'version': version}
                rows.append({'epoch': epoch, 'method': 'navigation/definition', 'phase': phase, 'site': site['id'], 'params': {'path': path, 'line': q['line'], 'character': q['character'], 'documentVersion': version}, 'response': {'result': result}, 'scoring': {'passed': True}})
        return {'id': 'compiler', 'rows': rows, 'passed': True, 'failures': []}

    def finalize(self, report):
        errors = finalize_reports([report], {'compiler': self.root}, [self.suite])
        return not errors and report['passed']

    def test_empty_missing_duplicate_corpus_cannot_pass(self):
        self.assertTrue(report_set_errors([], []))
        self.assertTrue(report_set_errors([], ['compiler']))
        self.assertTrue(report_set_errors([{'id': 'grace'}], ['compiler']))
        self.assertTrue(report_set_errors([{'id': 'compiler'}, {'id': 'compiler'}], ['compiler']))

    def test_manifest_corruption_does_not_reduce_denominator(self):
        mutations = []
        d = copy.deepcopy(self.manifest); d['suites'] = []; mutations.append(d)
        d = copy.deepcopy(self.manifest); d['suites'].pop(); mutations.append(d)
        d = copy.deepcopy(self.manifest); d['suites'][0] = copy.deepcopy(d['suites'][1]); mutations.append(d)
        d = copy.deepcopy(self.manifest); d['suites'][1]['required'].pop(); mutations.append(d)
        d = copy.deepcopy(self.manifest); d['suites'][0]['required'][0]['require'] = 'useful'; mutations.append(d)
        for changed in mutations:
            with self.assertRaises(Blocked): validate_contract(changed)

    def test_known_manifest_cannot_drop_probe(self):
        data = json.loads((Path(__file__).parent/'engine-corpus/known-failures.json').read_text())
        validate_known_contract(data)
        data['compiler']['cases'].pop()
        with self.assertRaises(Blocked): validate_known_contract(data)

    def test_complete_obligations_pass(self):
        report = self.report(); self.assertTrue(self.finalize(report)); self.assertEqual(report['obligationAudit']['observed'], 12); self.assertEqual(report['obligationAudit']['requiredResultObserved'], 8)

    def test_missing_phase_cannot_be_offset_by_extra_success(self):
        report = self.report(); report['rows'] = [r for r in report['rows'] if r.get('phase') != 'dirty']
        extra = copy.deepcopy(next(r for r in report['rows'] if r.get('phase') == 'ready')); extra['phase'] = 'known-failure-probe'; report['rows'].append(extra)
        self.assertFalse(self.finalize(report))

    def test_unscored_required_answer_fails(self):
        report = self.report(); next(r for r in report['rows'] if r.get('phase') == 'dirty').pop('scoring')
        self.assertFalse(self.finalize(report))

    def test_wrong_target_fails_despite_empty_failure_list(self):
        report = self.report(); row = next(r for r in report['rows'] if r.get('phase') == 'dirty')
        row['response']['result']['locations'] = [OLD]
        row['scoring'] = score(row['response'], GOLD, 'useful')
        self.assertFalse(row['scoring']['passed']); self.assertEqual(report['failures'], [])
        self.assertFalse(self.finalize(report))

    def test_forged_success_flag_cannot_hide_wrong_target(self):
        report = self.report(); row = next(r for r in report['rows'] if r.get('phase') == 'ready'); row['response']['result']['locations'] = [OLD]
        self.assertTrue(row['scoring']['passed']); self.assertFalse(self.finalize(report))

    def test_each_compiler_lifecycle_remains_individually_required(self):
        for site in ['local-array-pattern-part', 'tuple-match-idx']:
            for phase in ['dirty', 'closed-restored', 'ready']:
                report = self.report()
                report['rows'] = [r for r in report['rows'] if not (r.get('site') == site and r.get('phase') == phase)]
                self.assertFalse(self.finalize(report))

    def test_promotion_preserves_known_tuple_oracle_and_both_controls(self):
        known = json.loads((Path(__file__).parent/'engine-corpus/known-failures.json').read_text())
        old = next(s for s in known['compiler']['cases'] if s['id'] == 'tuple-match-idx')
        new = next(s for s in self.suite['required'] if s['id'] == 'tuple-match-idx')
        self.assertEqual({k: old[k] for k in new}, new)
        self.assertEqual([c['site'] for c in lifecycle_controls(self.suite)], ['local-array-pattern-part', 'tuple-match-idx'])
        for index in [0, 1]:
            broken = copy.deepcopy(self.manifest)
            if index == 0: broken['suites'][2]['lifecycle'] = broken['suites'][2]['additionalLifecycle'][0]
            else: broken['suites'][2]['additionalLifecycle'] = []
            with self.assertRaises(Blocked): validate_contract(broken)

    def test_missing_initial_or_demand_response_fails(self):
        for phase in ['first-response', 'discovery-trigger']:
            report = self.report(); report['rows'] = [r for r in report['rows'] if r.get('phase') != phase]
            self.assertFalse(self.finalize(report))

    def test_unscored_initial_response_fails(self):
        report = self.report(); next(r for r in report['rows'] if r.get('phase') == 'first-response').pop('scoring')
        self.assertFalse(self.finalize(report))

    def test_known_failure_probe_cannot_silently_disappear(self):
        report = self.report()
        errors = finalize_reports([report], {'compiler': self.root}, [self.suite], {'compiler': {'cases': [{'id': 'missing-probe'}]}})
        self.assertEqual(errors, []); self.assertFalse(report['passed'])
        self.assertIn('Known-failure probe observation set incomplete or duplicated', report['failures'])

    def test_loss_of_case_or_readiness_fails(self):
        for mode in ['case', 'readiness']:
            report = self.report()
            report['rows'] = [r for r in report['rows'] if not (r.get('phase') == 'ready' if mode == 'case' else r.get('method') == 'workspace/stats')]
            self.assertFalse(self.finalize(report))


class ReferenceGateTests(unittest.TestCase):
    @staticmethod
    def proof(version):
        return {'documentVersion': version, 'proofScope': 'document', 'source': 'local-lexical', 'documentProofId': 'unit-proof', 'sourceHash': 'unit-source-hash', 'proof': {'algorithm': 'unit-algorithm'}}

    def setUp(self):
        data = json.loads((Path(__file__).parent/'engine-corpus/manifest.json').read_text())
        self.suite = data['suites'][2]; self.case = copy.deepcopy(self.suite['references'][0]); self.root = Path('/portable-reference-source')
        # Synthetic transport text witnesses only; no F# source execution.
        self.texts = {'original': 'original', 'dirty': 'dirty', 'rebind': 'rebind'}
        self.case['textHashes'] = {k: hashlib.sha256(v.encode()).hexdigest() for k, v in self.texts.items()}
        self.suite['references'] = [self.case]

    def report(self):
        rows = []; path = str(self.root/self.case['query']['path'])
        for epoch, phases in [('fresh', ['first', 'ready', 'dirty', 'stale', 'rebind', 'closed-restored']), ('restart', ['first', 'ready'])]:
            for phase in phases:
                text_kind = 'dirty' if phase in ('dirty', 'stale') else 'rebind' if phase == 'rebind' else 'original'
                version = 2 if phase in ('dirty', 'stale') else 3 if phase == 'rebind' else 1
                rows.append({'epoch': epoch, 'method': 'document/open', 'params': {'path': path, 'version': version, 'text': self.texts[text_kind]}, 'response': {'result': {}}})
                if phase != 'first':
                    rows.append({'epoch': epoch, 'method': 'workspace/stats', 'response': {'result': {'background': {'phase': 'ready', 'discoveryComplete': True, 'indexComplete': True}}}})
                variant = reference_variant(self.case, phase); q = variant['query']
                for include in (True, False):
                    expected = [location(self.root, t) for t in variant['expectedReferences' if include else 'expectedReferencesWithoutDeclaration']]
                    response = {'error': {'message': 'stale version'}} if phase == 'stale' else {'result': {'completeness': 'exact', 'locations': expected, 'version': version, **self.proof(version)}}
                    rows.append({'epoch': epoch, 'method': 'navigation/references', 'phase': 'reference', 'referencePhase': phase, 'site': self.case['id'], 'params': {'path': path, 'line': q['line'], 'character': q['character'], 'documentVersion': version-1 if phase == 'stale' else version, 'includeDeclaration': include}, 'response': response, 'scoring': {'passed': True}})
        return {'id': 'compiler', 'rows': rows}

    def audit(self, report):
        return audit_reference_obligations(report, self.root, self.suite)

    def test_native_acknowledged_source_hash_mismatch_fails(self):
        report = self.report()
        report['rows'][0]['response']['result']['sourceHash'] = 'different-current-native-hash'
        self.assertTrue(self.audit(report)['failures'])

    def test_missing_or_forged_document_provenance_fails_raw_audit(self):
        for key, replacement in [('documentVersion', None), ('proofScope', 'project'), ('source', 'indexed'), ('documentProofId', ''), ('sourceHash', ''), ('proof', {})]:
            report = self.report()
            row = next(r for r in report['rows'] if r['method'] == 'navigation/references')
            row['response']['result'][key] = replacement
            self.assertTrue(self.audit(report)['failures'], key)

    def test_exact_set_requires_every_range_no_extras_or_duplicates(self):
        expected = [GOLD, OLD]
        for actual in [[], [GOLD], [GOLD, GOLD], [GOLD, OLD, OLD]]:
            response = {'result': {'completeness': 'exact', 'locations': actual, 'version': 1, **self.proof(1)}}
            self.assertFalse(score_references(response, expected, 1)['passed'])
        self.assertTrue(score_references({'result': {'completeness': 'exact', 'locations': [OLD, GOLD], 'version': 1, **self.proof(1)}}, expected, 1)['passed'])

    def test_unavailable_or_candidates_never_satisfy_exact_reference_set(self):
        for value in [{'completeness': 'unavailable', 'locations': [], 'version': 1}, {'completeness': 'unavailable', 'candidates': [GOLD], 'candidateCompleteness': 'incomplete', 'version': 1}, {'completeness': 'exact', 'locations': [GOLD], 'candidates': [OLD], 'version': 1}]:
            self.assertFalse(score_references({'result': value}, [GOLD], 1)['passed'])

    def test_stale_version_never_exposes_complete_empty_or_locations(self):
        self.assertTrue(score_references({'error': {}}, [], 2, stale=True)['passed'])
        for value in [{}, {'completeness': 'exact', 'locations': []}, {'completeness': 'unavailable', 'locations': [GOLD]}]:
            self.assertFalse(score_references({'result': value}, [], 2, stale=True)['passed'])

    def test_original_and_rebind_contract_sets_cannot_be_reduced(self):
        data = json.loads((Path(__file__).parent/'engine-corpus/manifest.json').read_text()); suite = data['suites'][2]
        validate_reference_contract(suite)
        for key in ['expectedReferences', 'expectedReferencesWithoutDeclaration']:
            broken = copy.deepcopy(suite); broken['references'][0][key].pop()
            with self.assertRaises(Blocked): validate_reference_contract(broken)
        broken = copy.deepcopy(suite); broken['references'] = []
        with self.assertRaises(Blocked): validate_reference_contract(broken)

    def test_all_sixteen_protocol_observations_are_required(self):
        result = self.audit(self.report()); self.assertEqual(result['failures'], [])
        self.assertEqual((result['expected'], result['observed'], result['exactSetExpected'], result['staleRejectionExpected']), (16, 16, 14, 2))

    def test_every_phase_and_declaration_mode_individually_required(self):
        for phase in ['first', 'ready', 'dirty', 'stale', 'rebind', 'closed-restored']:
            for include in (True, False):
                report = self.report(); report['rows'] = [r for r in report['rows'] if not (r.get('referencePhase') == phase and r.get('params', {}).get('includeDeclaration') is include)]
                self.assertTrue(self.audit(report)['failures'])

    def test_wrong_set_with_success_flag_and_unscored_answer_fail(self):
        for mode in ['wrong', 'unscored']:
            report = self.report(); row = next(r for r in report['rows'] if r.get('referencePhase') == 'dirty')
            if mode == 'wrong': row['response']['result']['locations'] = []
            else: row.pop('scoring')
            self.assertTrue(self.audit(report)['failures'])

    def test_rebind_excludes_inner_homonyms(self):
        expected = [location(self.root, t) for t in self.case['rebind']['expectedReferences']]
        forbidden = reference_forbidden(self.root, self.case, 'rebind', 'fresh')
        inner = location(self.root, self.case['rebind']['excludedReferences'][0])
        self.assertIn(inner, forbidden)
        response = {'result': {'completeness': 'exact', 'locations': expected + [inner], 'version': 3}}
        self.assertFalse(score_references(response, expected, 3, forbidden)['passed'])

    def test_changed_text_or_version_witness_fails(self):
        for key, value in [('version', 99), ('text', 'not-the-acknowledged-variant')]:
            report = self.report(); row = next(r for r in report['rows'] if r.get('method') == 'document/open'); row['params'][key] = value
            self.assertTrue(self.audit(report)['failures'])


if __name__ == '__main__':
    unittest.main()
