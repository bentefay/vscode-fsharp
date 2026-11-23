#!/usr/bin/env python3
"""Portable, fail-closed, source-only native corpus gate. Never builds or prepares."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import tempfile
import time

HERE = Path(__file__).resolve().parent
MANIFEST = HERE / 'engine-corpus/manifest.json'
KNOWN = HERE / 'engine-corpus/known-failures.json'


# Deliberate contract changes require reviewing this list as well as the manifest.
# Extra coverage cannot silently replace any existing required obligation.
REQUIRED_CONTRACTS = {
    'grace': {'closed-lambda-range': 'exact', 'module_value_use_26_serialize': 'useful'},
    'fscheck': {'crossfile-common-takeWhilePlusLast': 'useful', 'local-tuple-match-res': 'exact', 'local-recursive-loop': 'exact'},
    'compiler': {'local-array-pattern-part': 'exact', 'tuple-match-idx': 'useful'},
}


class Blocked(Exception):
    pass


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def safe_path(root, relative):
    path = root / relative
    if Path(relative).is_absolute() or not path.resolve().is_relative_to(root.resolve()):
        raise Blocked('Path escapes supplied corpus root: ' + relative)
    return path


def inventory(root, policy):
    if not root.is_dir():
        raise Blocked('Missing source directory: ' + str(root))
    files = {}
    for path in sorted(root.rglob('*')):
        relative = path.relative_to(root)
        if any(p in policy['excludedDirectories'] for p in relative.parts):
            continue
        if path.is_symlink():
            raise Blocked('Symlink in corpus workspace: ' + str(relative))
        if path.is_file() and (path.suffix in policy['suffixes'] or path.name in policy['names']):
            files[relative.as_posix()] = digest(path)
    return {'count': len(files), 'sha256': hashlib.sha256(
        json.dumps(files, sort_keys=True, separators=(',', ':')).encode()).hexdigest()}


def validate(root, suite):
    actual = inventory(safe_path(root, suite['workspace']), suite['inventory'])
    if any(actual[k] != suite['inventory'][k] for k in ['count', 'sha256']):
        raise Blocked(suite['id'] + ': source inventory differs from frozen revision')
    actual_ancestors = {}
    workspace = safe_path(root, suite['workspace'])
    for parent in [root, *workspace.parents]:
        if not parent.is_relative_to(root):
            continue
        for path in parent.iterdir():
            if path.is_file() and (path.suffix in ['.props', '.targets'] or path.name in suite['inventory']['names']):
                actual_ancestors[path.relative_to(root).as_posix()] = digest(path)
    if actual_ancestors != suite['ancestorInputs']:
        raise Blocked(suite['id'] + ': enclosing input set/content differs from frozen source')
    for name, expected in {**suite['ancestorInputs'], suite['project']: suite['projectSha256']}.items():
        path = safe_path(root, name)
        if not path.is_file() or digest(path) != expected:
            raise Blocked(suite['id'] + ': missing/changed project or ancestor input: ' + name)
    for site in suite['required']:
        for entry, token in [(site['query'], site['query']['token']), (site['expected'], site['query']['token'])]:
            lines = safe_path(root, entry['path']).read_text(encoding='utf-8-sig').splitlines()
            if 'range' in entry:
                start, end = entry['range']['start'], entry['range']['end']
                if start['line'] != end['line']:
                    raise Blocked('Only single-line frozen identifier targets are supported')
                text = lines[start['line']].encode('utf-16-le')[2*start['character']:2*end['character']].decode('utf-16-le')
            else:
                text = lines[entry['line']].encode('utf-16-le')[2*entry['character']:].decode('utf-16-le')[:len(token)]
            if text != token:
                raise Blocked('Frozen literal span mismatch: ' + site['id'])
    for case in suite.get('references', []):
        reference_texts(root, case)
    return actual


def location(root, target):
    return {'uri': safe_path(root, target['path']).resolve().as_uri(), 'range': target['range']}


def score(response, expected, requirement, forbidden=(), expected_version=None):
    if 'error' in response:
        return {'passed': False, 'classification': 'protocol-error'}
    value = response.get('result', {})
    if expected_version is not None and (value.get('version') != expected_version or value.get('documentVersion', expected_version) != expected_version):
        return {'passed': False, 'classification': 'stale-or-missing-document-version'}
    exact = value.get('locations', [])
    candidates = value.get('candidates', [])
    likely = value.get('likelyCandidates', [])
    if any(old in exact + candidates + likely for old in forbidden):
        return {'passed': False, 'classification': 'stale-target'}
    if value.get('completeness') == 'exact':
        ok = exact == [expected] and not candidates and not likely
        return {'passed': ok, 'classification': 'exact' if ok else 'wrong-exact'}
    if exact:
        return {'passed': False, 'classification': 'unproven-exact-locations'}
    if candidates and value.get('candidateCompleteness') != 'incomplete':
        return {'passed': False, 'classification': 'unlabelled-candidates'}
    if any(x not in candidates for x in likely):
        return {'passed': False, 'classification': 'invalid-likely-subset'}
    hit = expected in candidates
    return {'passed': requirement == 'useful' and hit,
            'classification': 'known-candidate' if hit else 'unavailable',
            'candidateCount': len(candidates), 'knownRank': candidates.index(expected)+1 if hit else None}



def lifecycle_controls(suite):
    return [suite['lifecycle'], *suite.get('additionalLifecycle', [])]


def validate_contract(manifest):
    suites = manifest.get('suites', [])
    ids = [s.get('id') for s in suites]
    if manifest.get('schemaVersion') != 1 or len(ids) != len(REQUIRED_CONTRACTS) or set(ids) != set(REQUIRED_CONTRACTS):
        raise Blocked('Manifest must contain each frozen corpus exactly once')
    for suite in suites:
        validate_reference_contract(suite)
        cases = suite.get('required', [])
        actual = {s.get('id'): s.get('require') for s in cases}
        if len(cases) != len(actual) or actual != REQUIRED_CONTRACTS[suite['id']]:
            raise Blocked(suite['id'] + ': required case IDs/strength differ from frozen contract')
        if any(s.get('operation') != 'navigation/definition' for s in cases):
            raise Blocked('Required definition operation changed')
        controls = lifecycle_controls(suite)
        expected_controls = {'grace': ['module_value_use_26_serialize'], 'fscheck': ['local-tuple-match-res'], 'compiler': ['local-array-pattern-part', 'tuple-match-idx']}[suite['id']]
        if [c.get('site') for c in controls] != expected_controls:
            raise Blocked('Missing, duplicated, or replaced lifecycle control')
        for control in controls:
            if control.get('site') not in actual or control.get('lines') != 8 or control.get('prefix', '').count('\n') != 8:
                raise Blocked('Missing or changed dirty/close/restart control')


def validate_known_contract(known):
    expected = {'tuple-match-idx', 'crossfile-TaggedText-mkTag', 'signature-RichText-IsEmpty',
                'record-SHAStream-pos', 'qualified-union-While', 'generic-InterruptibleLazy-Value',
                'partial-active-pattern-RecoverableException', 'custom-operator-SHA1-shift'}
    cases = known.get('compiler', {}).get('cases', [])
    ids = [s.get('id') for s in cases]
    if known.get('schemaVersion') != 1 or len(ids) != len(expected) or set(ids) != expected:
        raise Blocked('Known compiler probe set differs from frozen contract')


def report_set_errors(reports, expected_ids):
    actual = [s.get('id') for s in reports]
    if not expected_ids or len(actual) != len(expected_ids) or set(actual) != set(expected_ids):
        return ['Observed corpus set does not match nonempty selected corpus set']
    return []


def audit_obligations(report, root, suite):
    """Re-score each required raw answer, not aggregate counts or stored passed flags."""
    expected = {}
    for epoch in ['fresh', 'restart']:
        first = suite['required'][0]
        expected[(epoch, 'first-response', first['id'])] = copy.deepcopy(first)
        trigger = suite['discoveryTrigger']
        if trigger['id'] != first['id']:
            expected[(epoch, 'discovery-trigger', trigger['id'])] = copy.deepcopy(trigger)
        for site in suite['required']:
            expected[(epoch, 'ready', site['id'])] = copy.deepcopy(site)
    shifted_controls = {}
    for control in lifecycle_controls(suite):
        original = next(s for s in suite['required'] if s['id'] == control['site'])
        shifted = copy.deepcopy(original)
        shifted['expected']['range']['start']['line'] += control['lines']
        shifted['expected']['range']['end']['line'] += control['lines']
        if shifted['query']['path'] == original['expected']['path']:
            shifted['query']['line'] += control['lines']
        expected[('fresh', 'dirty', original['id'])] = shifted
        expected[('fresh', 'closed-restored', original['id'])] = original
        shifted_controls[original['id']] = (original, shifted)
    observed = {}; failures = []; versions = {}; ready = set()
    for row in report.get('rows', []):
        epoch = row.get('epoch'); method = row.get('method'); params = row.get('params') or {}
        response = row.get('response', {})
        if method in ('document/open', 'document/change') and 'error' not in response:
            versions[(epoch, params.get('path'))] = params.get('version')
        elif method == 'document/close' and 'error' not in response:
            versions.pop((epoch, params.get('path')), None)
        if method == 'workspace/stats':
            value = response.get('result', {}); bg = value.get('background', {})
            if bg.get('phase') == 'ready' and bg.get('discoveryComplete') is True and bg.get('indexComplete') is True and not value.get('indexErrors'):
                ready.add(epoch)
        if row.get('phase') not in ('ready', 'dirty', 'closed-restored', 'first-response', 'discovery-trigger'):
            continue
        key = (epoch, row.get('phase'), row.get('site'))
        observed[key] = observed.get(key, 0) + 1
        if key not in expected:
            failures.append('Unexpected required-phase observation: ' + str(key)); continue
        site = expected[key]; q = site['query']; version = versions.get((epoch, params.get('path')))
        required_result = row['phase'] in ('ready', 'dirty', 'closed-restored')
        if required_result and epoch not in ready:
            failures.append('Required observation before verified ready: ' + str(key))
        if method != site['operation'] or params.get('path') != str(safe_path(root, q['path'])) or params.get('line') != q['line'] or params.get('character') != q['character'] or version is None or params.get('documentVersion') != version:
            failures.append('Required request/origin acknowledgement differs: ' + str(key))
        forbidden = []
        if site['id'] in shifted_controls:
            original, shifted = shifted_controls[site['id']]
            if row['phase'] == 'dirty':
                forbidden = [location(root, original['expected'])]
            elif row['phase'] == 'closed-restored' or epoch == 'restart':
                forbidden = [location(root, shifted['expected'])]
        rescored = score(response, location(root, site['expected']), site['require'], forbidden, version)
        implementation = site.get('implementationDeclaration')
        if row['phase'] == 'discovery-trigger' and implementation and rescored['classification'] == 'wrong-exact' and response.get('result', {}).get('locations') == [location(root, implementation)]:
            rescored = {'passed': False, 'classification': 'implementation-target-not-signature-policy'}
        unscored = 'passed' not in row.get('scoring', {})
        unsafe = rescored['classification'] in ('protocol-error', 'wrong-exact', 'stale-target', 'unproven-exact-locations', 'unlabelled-candidates', 'invalid-likely-subset', 'stale-or-missing-document-version')
        if unscored or unsafe or (required_result and (not rescored['passed'] or row.get('scoring', {}).get('passed') is not True)):
            failures.append('Expected result absent/unscored/incorrect: ' + str(key) + ': ' + rescored['classification'])
    for key in expected:
        if observed.get(key) != 1:
            failures.append('Required phase must occur exactly once: ' + str(key))
    return {'expected': len(expected), 'observed': sum(observed.values()), 'requiredResultExpected': sum(k[1] in ('ready', 'dirty', 'closed-restored') for k in expected), 'requiredResultObserved': sum(v for k, v in observed.items() if k[1] in ('ready', 'dirty', 'closed-restored')), 'failures': failures}



def reference_texts(root, case):
    clean = safe_path(root, case['query']['path']).read_text(encoding='utf-8-sig')
    lines = clean.splitlines(keepends=True); rebind = case['rebind']; line = rebind['line']
    if lines[line].rstrip('\r\n') != rebind['originalLine']:
        raise Blocked('Reference rebind original source line differs')
    ending = '\r\n' if lines[line].endswith('\r\n') else '\n' if lines[line].endswith('\n') else ''
    lines[line] = rebind['replacementLine'] + ending
    texts = {'original': clean, 'dirty': case['prefix'] + clean, 'rebind': ''.join(lines)}
    for name, text in texts.items():
        if hashlib.sha256(text.encode()).hexdigest() != case['textHashes'][name]:
            raise Blocked('Reference variant text hash differs: ' + name)
    for phase in ('first', 'dirty', 'rebind'):
        variant = reference_variant(case, phase); text = texts['original' if phase == 'first' else phase]; source_lines = text.splitlines()
        q = variant['query']
        if not source_lines[q['line']].encode('utf-16-le')[2*q['character']:].decode('utf-16-le').startswith(q['token']):
            raise Blocked('Reference query literal differs: ' + phase)
        for target in variant['expectedReferences'] + variant['expectedReferencesWithoutDeclaration']:
            a, z = target['range']['start'], target['range']['end']
            if a['line'] != z['line'] or source_lines[a['line']].encode('utf-16-le')[2*a['character']:2*z['character']].decode('utf-16-le') != target['text']:
                raise Blocked('Reference target literal differs: ' + phase)
    return texts


def reference_variant(case, phase):
    variant = copy.deepcopy(case)
    if phase in ('dirty', 'stale'):
        variant['query']['line'] += case['lines']
        for key in ('expectedReferences', 'expectedReferencesWithoutDeclaration'):
            for item in variant[key]:
                item['range']['start']['line'] += case['lines']
                item['range']['end']['line'] += case['lines']
    elif phase == 'rebind':
        variant.update(copy.deepcopy(case['rebind']))
    return variant


def reference_forbidden(root, case, phase, epoch):
    original = case['expectedReferences']
    shifted = reference_variant(case, 'dirty')['expectedReferences']
    rhs = case['rebind']['expectedReferencesWithoutDeclaration']
    if phase in ('dirty', 'stale'):
        targets = original
    elif phase == 'rebind':
        targets = case['expectedReferencesWithoutDeclaration'] + case['rebind']['excludedReferences']
    elif phase == 'closed-restored' or epoch == 'restart':
        targets = shifted + rhs
    else:
        targets = []
    return [location(root, t) for t in targets]


def score_references(response, expected, version, forbidden=(), stale=False):
    if 'error' in response:
        return {'passed': stale, 'classification': 'stale-rejected' if stale else 'protocol-error'}
    value = response.get('result', {})
    locations = value.get('locations', [])
    candidates = value.get('candidates', []) + value.get('likelyCandidates', [])
    if stale:
        ok = value.get('completeness') in ('unavailable', 'incomplete') and not locations and not candidates
        return {'passed': ok, 'classification': 'stale-rejected' if ok else 'stale-request-exposed-result'}
    if value.get('version') != version or value.get('documentVersion') != version:
        return {'passed': False, 'classification': 'stale-or-missing-document-version'}
    if value.get('proofScope') != 'document' or value.get('source') != 'local-lexical' or not all(isinstance(x, str) and x.strip() for x in (value.get('documentProofId'), value.get('sourceHash'), value.get('proof', {}).get('algorithm'))):
        return {'passed': False, 'classification': 'missing-or-wrong-document-proof'}
    if any(t in locations + candidates for t in forbidden):
        return {'passed': False, 'classification': 'stale-or-shadowed-reference'}
    canonical = lambda items: sorted(json.dumps(x, sort_keys=True) for x in items)
    ok = value.get('completeness') == 'exact' and not candidates and canonical(locations) == canonical(expected)
    return {'passed': ok, 'classification': 'exact-reference-set' if ok else 'missing-or-incorrect-reference-set', 'expectedCount': len(expected), 'actualCount': len(locations)}


def validate_reference_contract(suite):
    cases = suite.get('references', [])
    expected_ids = ['references-local-part'] if suite['id'] == 'compiler' else []
    if [s.get('id') for s in cases] != expected_ids:
        raise Blocked('Required reference case set differs from frozen contract')
    for case in cases:
        if case.get('operation') != 'navigation/references' or case.get('require') != 'exact-complete-set' or case.get('includeDeclarationModes') != [True, False]:
            raise Blocked('Reference operation/strength/declaration modes changed')
        path = 'src/Compiler/Facilities/RichText.fs'
        spans = lambda items: [(x['path'], x['range']['start']['line'], x['range']['start']['character'], x['range']['end']['line'], x['range']['end']['character']) for x in items]
        if spans(case['expectedReferences']) != [(path, 15, 13, 15, 17), (path, 15, 24, 15, 28)] or spans(case['expectedReferencesWithoutDeclaration']) != [(path, 15, 24, 15, 28)]:
            raise Blocked('Frozen complete part reference set changed')
        if spans(case['rebind']['expectedReferences']) != [(path, 15, 13, 15, 17), (path, 15, 36, 15, 40)] or spans(case['rebind']['expectedReferencesWithoutDeclaration']) != [(path, 15, 36, 15, 40)]:
            raise Blocked('Frozen rebind reference set changed')
        if case.get('lines') != 8 or case.get('prefix', '').count('\n') != 8:
            raise Blocked('Reference prefix shift changed')


def audit_reference_obligations(report, root, suite):
    cases = {s['id']: s for s in suite.get('references', [])}
    phases = {'fresh': ['first', 'ready', 'dirty', 'stale', 'rebind', 'closed-restored'], 'restart': ['first', 'ready']}
    expected = {(epoch, phase, sid, include) for epoch, names in phases.items() for phase in names for sid in cases for include in (True, False)}
    observed = {}; failures = []; versions = {}; texts = {}; native_hashes = {}; ready = set()
    for row in report.get('rows', []):
        epoch = row.get('epoch'); method = row.get('method'); params = row.get('params') or {}; response = row.get('response', {})
        keypath = (epoch, params.get('path'))
        if method in ('document/open', 'document/change') and 'error' not in response:
            versions[keypath] = params.get('version'); texts[keypath] = hashlib.sha256(params.get('text', '').encode()).hexdigest()
            native_hashes[keypath] = response.get('result', {}).get('sourceHash')
        elif method == 'document/close' and 'error' not in response:
            versions.pop(keypath, None); texts.pop(keypath, None)
        if method == 'workspace/stats':
            v = response.get('result', {}); bg = v.get('background', {})
            if bg.get('phase') == 'ready' and bg.get('discoveryComplete') is True and bg.get('indexComplete') is True and not v.get('indexErrors'):
                ready.add(epoch)
        if row.get('phase') != 'reference':
            continue
        phase = row.get('referencePhase'); include = params.get('includeDeclaration'); key = (epoch, phase, row.get('site'), include)
        observed[key] = observed.get(key, 0) + 1
        if key not in expected or type(include) is not bool:
            failures.append('Unexpected reference observation: ' + str(key)); continue
        case = cases[row['site']]; variant = reference_variant(case, phase); q = variant['query']; current = versions.get(keypath); stale = phase == 'stale'
        desired_text = 'dirty' if phase in ('dirty', 'stale') else 'rebind' if phase == 'rebind' else 'original'
        if phase != 'first' and epoch not in ready:
            failures.append('Reference phase before verified ready: ' + str(key))
        if method != 'navigation/references' or params.get('path') != str(safe_path(root, q['path'])) or params.get('line') != q['line'] or params.get('character') != q['character'] or current is None or params.get('documentVersion') != (current-1 if stale else current) or texts.get(keypath) != case['textHashes'][desired_text]:
            failures.append('Reference request/version/text differs: ' + str(key))
        if not stale and native_hashes.get(keypath) is not None and response.get('result', {}).get('sourceHash') != native_hashes[keypath]:
            failures.append('Reference source hash differs from acknowledged native hash: ' + str(key))
        targets = [location(root, t) for t in variant['expectedReferences' if include else 'expectedReferencesWithoutDeclaration']]
        scored = score_references(response, targets, current, reference_forbidden(root, case, phase, epoch), stale)
        if not scored['passed'] or row.get('scoring', {}).get('passed') is not True:
            failures.append('Required reference result absent/unscored/incorrect: ' + str(key) + ': ' + scored['classification'])
    for key in expected:
        if observed.get(key) != 1:
            failures.append('Reference phase must occur exactly once: ' + str(key))
    return {'expected': len(expected), 'observed': sum(observed.values()), 'exactSetExpected': sum(k[1] != 'stale' for k in expected), 'staleRejectionExpected': sum(k[1] == 'stale' for k in expected), 'failures': failures}


def finalize_reports(reports, roots, selected, known=None):
    errors = report_set_errors(reports, [s['id'] for s in selected])
    if errors:
        return errors
    contracts = {s['id']: s for s in selected}
    for report in reports:
        audit = audit_obligations(report, roots[report['id']], contracts[report['id']])
        report['obligationAudit'] = audit
        refs_audit = audit_reference_obligations(report, roots[report['id']], contracts[report['id']])
        report['referenceObligationAudit'] = refs_audit
        report.setdefault('failures', []).extend(refs_audit['failures'])
        report.setdefault('failures', []).extend(audit['failures'])
        if known is not None:
            ids = [s['id'] for s in known.get(report['id'], {}).get('cases', [])]
            observed = [r.get('site') for r in report.get('rows', []) if r.get('phase') == 'known-failure-probe' and r.get('epoch') == 'fresh' and 'passed' in r.get('scoring', {})]
            summaries = [r.get('id') for r in report.get('knownFailures', [])]
            if sorted(ids) != sorted(observed) or sorted(ids) != sorted(summaries):
                report['failures'].append('Known-failure probe observation set incomplete or duplicated')
        report['passed'] = not report['failures']
    return []


def run_suite(binary, root, suite, cache, known, timeout):
    from run import Session
    report = {'id': suite['id'], 'rows': [], 'failures': [], 'knownFailures': []}
    def fail(message):
        report['failures'].append(message)
    for epoch in ['fresh', 'restart']:
        session = Session([str(binary), 'daemon'], timeout=timeout, sample_interval=.2)
        versions = {}
        def call(method, params=None, allow_error=False):
            response, _ = session.call(method, params)
            report['rows'].append({'epoch': epoch, 'method': method, 'params': params, 'response': response})
            if 'error' in response and not allow_error:
                raise RuntimeError(method + ': ' + str(response['error']))
            return response
        def opened(path):
            if path not in versions:
                call('document/open', {'path': str(path), 'version': 1, 'text': path.read_text(encoding='utf-8-sig')})
                versions[path] = 1
        def query(site, phase, forbidden=(), required=True):
            path = safe_path(root, site['query']['path']); opened(path)
            q = site['query']; response = call(site['operation'], {'path': str(path), 'line': q['line'], 'character': q['character'], 'documentVersion': versions[path]})
            result = score(response, location(root, site['expected']), site['require'], forbidden, versions[path])
            implementation = site.get('implementationDeclaration')
            if result['classification'] == 'wrong-exact' and phase in ('known-failure-probe', 'discovery-trigger') and implementation and response.get('result', {}).get('completeness') == 'exact' and response['result'].get('locations') == [location(root, implementation)]:
                result = {'passed': False, 'classification': 'implementation-target-not-signature-policy'}
            report['rows'][-1].update(site=site['id'], phase=phase, scoring=result)
            if required and not result['passed']:
                fail(epoch + '/' + phase + '/' + site['id'] + ': ' + result['classification'])
            # Cold responses may be incomplete, but false exacts/stale labels never tolerated.
            if not required and result['classification'] in ('wrong-exact', 'stale-target', 'unproven-exact-locations', 'unlabelled-candidates', 'invalid-likely-subset', 'stale-or-missing-document-version'):
                fail(epoch + '/' + phase + ': ' + result['classification'])
            return result
        def reference_query(case, phase, include):
            variant = reference_variant(case, phase); q = variant['query']; path = safe_path(root, q['path']); opened(path)
            stale = phase == 'stale'; version = versions[path]
            response = call('navigation/references', {'path': str(path), 'line': q['line'], 'character': q['character'], 'documentVersion': version-1 if stale else version, 'includeDeclaration': include}, allow_error=stale)
            expected = [location(root, t) for t in variant['expectedReferences' if include else 'expectedReferencesWithoutDeclaration']]
            result = score_references(response, expected, version, reference_forbidden(root, case, phase, epoch), stale)
            report['rows'][-1].update(site=case['id'], phase='reference', referencePhase=phase, scoring=result)
            if not result['passed']:
                fail(epoch + '/reference/' + phase + '/' + str(include) + ': ' + result['classification'])
        def reference_pair(case, phase):
            for include in (True, False):
                reference_query(case, phase, include)

        try:
            call('workspace/open', {'root': str(safe_path(root, suite['workspace'])), 'mode': 'indexed', 'cacheDir': str(cache)})
            for case in suite.get('references', []):
                reference_pair(case, 'first')
            query(suite['required'][0], 'first-response', required=False)
            trigger = suite['discoveryTrigger']
            if trigger['id'] != suite['required'][0]['id']:
                query(trigger, 'discovery-trigger', required=False)
            deadline = time.monotonic() + timeout
            while True:
                stats = call('workspace/stats')['result']; bg = stats.get('background', {})
                if bg.get('phase') == 'ready' and bg.get('discoveryComplete') is True and bg.get('indexComplete') is True:
                    if stats.get('indexErrors'):
                        fail(epoch + ': index errors')
                    break
                if bg.get('phase') in ('limited', 'error') or time.monotonic() >= deadline:
                    raise RuntimeError('Discovery/index not ready: ' + str(bg))
                time.sleep(.1)
            for site in suite['required']:
                forbidden = []
                control = next((c for c in lifecycle_controls(suite) if c['site'] == site['id']), None)
                if epoch == 'restart' and control:
                    previous = copy.deepcopy(site['expected'])
                    previous['range']['start']['line'] += control['lines']
                    previous['range']['end']['line'] += control['lines']
                    forbidden = [location(root, previous)]
                query(site, 'ready', forbidden)
            for case in suite.get('references', []):
                reference_pair(case, 'ready')
            if epoch == 'fresh':
                for site in known:
                    outcome = query(site, 'known-failure-probe', required=False)
                    report['knownFailures'].append({'id': site['id'], 'observed': outcome, 'gateStatus': 'not-a-required-pass'})
                for control in lifecycle_controls(suite):
                    original = next(s for s in suite['required'] if s['id'] == control['site'])
                    provider = safe_path(root, original['expected']['path']); opened(provider)
                    clean = provider.read_text(encoding='utf-8-sig'); changed = control['prefix'] + clean
                    versions[provider] += 1
                    call('document/change', {'path': str(provider), 'version': versions[provider], 'text': changed})
                    shifted = copy.deepcopy(original)
                    shifted['expected']['range']['start']['line'] += control['lines']; shifted['expected']['range']['end']['line'] += control['lines']
                    if shifted['query']['path'] == original['expected']['path']:
                        shifted['query']['line'] += control['lines']
                    query(shifted, 'dirty', [location(root, original['expected'])])
                    call('document/close', {'path': str(provider)}); versions.pop(provider)
                    query(original, 'closed-restored', [location(root, shifted['expected'])])
            if epoch == 'fresh':
                for case in suite.get('references', []):
                    texts = reference_texts(root, case); path = safe_path(root, case['query']['path']); opened(path)
                    versions[path] += 1
                    call('document/change', {'path': str(path), 'version': versions[path], 'text': texts['dirty']})
                    reference_pair(case, 'dirty')
                    reference_pair(case, 'stale')
                    versions[path] += 1
                    call('document/change', {'path': str(path), 'version': versions[path], 'text': texts['rebind']})
                    reference_pair(case, 'rebind')
                    call('document/close', {'path': str(path)}); versions.pop(path)
                    reference_pair(case, 'closed-restored')
            stats = call('workspace/stats')['result']
            for key in ['compilerLaunches', 'helperStarts', 'decompilerStarts']:
                if stats.get(key) != 0:
                    fail(epoch + ': implicit tool counter nonzero/missing: ' + key)
        except Exception as exc:
            fail(epoch + ': ' + str(exc))
        finally:
            try:
                session.close()
            except Exception as exc:
                fail(epoch + ": cleanup failed: " + str(exc))
        if epoch == 'fresh' and not any(p.is_file() and p.stat().st_size for p in cache.rglob('*.sqlite')):
            fail('No persisted SQLite cache before restart')
    report['passed'] = not report['failures']
    return report


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True)
    for name in ['grace', 'fscheck', 'compiler']:
        p.add_argument('--'+name+'-root', type=Path)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--suite', action='append', choices=['grace', 'fscheck', 'compiler'])
    p.add_argument('--timeout', type=float, default=30)
    args = p.parse_args(argv)
    report = {'schemaVersion': 1, 'status': 'blocked', 'manifestSha256': digest(MANIFEST), 'knownFailuresSha256': digest(KNOWN), 'runnerSha256': digest(__file__), 'suites': [], 'limitations': ['Native correctness/availability only, not UI or timing benchmark.', 'Definition edits are layout-only; the part reference control additionally checks a nested nonrecursive-let rebind.', 'Known-failure probes are never counted as required passes.']}
    try:
        manifest = json.loads(MANIFEST.read_text()); known = json.loads(KNOWN.read_text())
        validate_contract(manifest)
        validate_known_contract(known)
        selected = [s for s in manifest['suites'] if not args.suite or s['id'] in args.suite]
        report['selectedSuites'] = [s['id'] for s in selected]
        report['allCorporaSelected'] = len(selected) == len(manifest['suites'])
        roots = {}
        for suite in selected:
            supplied = getattr(args, suite['id']+'_root')
            if supplied is None:
                raise Blocked('Missing explicit --'+suite['id']+'-root')
            roots[suite['id']] = supplied.resolve()
            validate(roots[suite['id']], suite)
        binary = args.binary.resolve()
        if not binary.is_file():
            raise Blocked('Missing native binary')
        report['binarySha256'] = digest(binary)
        with tempfile.TemporaryDirectory(prefix='fsnav-corpus-gate-') as tmp:
            for suite in selected:
                cache = Path(tmp)/suite['id']; cache.mkdir()
                report['suites'].append(run_suite(binary, roots[suite['id']], suite, cache, known.get(suite['id'], {}).get('cases', []), args.timeout))
        for suite in selected:
            validate(roots[suite['id']], suite)
        if digest(binary) != report['binarySha256'] or digest(MANIFEST) != report['manifestSha256'] or digest(KNOWN) != report['knownFailuresSha256'] or digest(__file__) != report['runnerSha256']:
            raise Blocked('Inputs changed during run')
        report['inputsUnchanged'] = True
        report['corpusSetErrors'] = finalize_reports(report['suites'], roots, selected, known)
        report['status'] = 'passed' if not report['corpusSetErrors'] and all(s['passed'] for s in report['suites']) else 'failed'
    except (Blocked, OSError, ValueError, IndexError) as exc:
        report['status'] = 'blocked'
        report['blocker'] = str(exc)
    except Exception as exc:
        report['status'] = 'failed'
        report['error'] = type(exc).__name__ + ': ' + str(exc)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps({'status': report['status'], 'output': str(args.output), 'failures': [f for s in report['suites'] for f in s['failures']], 'blocker': report.get('blocker')}))
    return {'passed': 0, 'failed': 1, 'blocked': 2}[report['status']]


if __name__ == '__main__':
    raise SystemExit(main())
