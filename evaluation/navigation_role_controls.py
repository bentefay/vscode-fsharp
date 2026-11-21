#!/usr/bin/env python3
"""SDK/FCS differential controls for indexed argument roles, signatures and edits.

Run through scripts/with-local-tools (or the project Nix shell):
    python3 evaluation/navigation_role_controls.py --binary /abs/fsharp-navigator \
        --output-dir /tmp/new-run [--previous-binary /abs/older-fsharp-navigator]

Pass --helper with the built FSharpNavigator.Semantic.dll for prepared receiver controls.

The native runs use simple explicit compile lists and acknowledged buffers.
The compiler runs separately. No semantic snapshot or helper is supplied to the daemon.
"""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'semantic/tests'))
from resolution_states import Native
import smoke
from smoke import project

ORACLE = ROOT / 'evaluation/experiments/local-lexical-proof/oracle.fsx'
STATIC = 'type C = static member M(x:bool) = x'
BOTH = STATIC + '\nlet g a = a'
GETTER = 'type C() = member _.Item with get(x:bool) = x'

# Only the marked identifiers are queried. Their meaning comes from the live checker.
# The last column identifies simple positives for which refusal is a regression.
CASES = [
    ('ordinary', BOTH, 'let caller «x» = g(«x»=«x»)', True),
    ('named', BOTH, 'let caller «x» = C.M(«x»=«x»)', False),
    ('spaced', STATIC, 'let caller «x» = C.M («x»=«x»)', False),
    ('functionFormal', '', 'let caller (g:bool -> bool) «x» = g(«x»=«x»)', True),
    ('qualified', 'module A =\n    let g a=a', 'let caller «x» = A.g(«x»=«x»)', False),
    ('valueAlias', 'let g a=a\nlet f=g', 'let caller «x» = f(«x»=«x»)', True),
    ('methodValue', STATIC + '\nlet g=C.M', 'let caller «x» = g(«x»=«x»)', True),
    ('grouped', STATIC, 'let caller «x» = C.M((«x»)=«x»)', True),
    ('qualifiedOperand', STATIC + '\nmodule A =\n    let x=0',
     'let caller «x» = C.M(A.«x»=«x»)', False),
    ('methodParen', STATIC, 'let caller «x» = (C.M)(«x»=«x»)', False),
    ('curried', 'type C = static member M(y:bool)(x:bool) = x',
     'let caller «x» = C.M(false)(«x»=«x»)', False),
    ('ctorAlias', 'type C(x:int) = member _.P=x\ntype A=C',
     'let caller «x» = A(«x»=«x»).P', False),
    ('instance', 'type C() = member _.M(x:int) = x',
     'let caller (c:C) «x» = c.M(«x»=«x»)', False),
    ('generic', "type C = static member M<'T>(x:'T) = x",
     'let caller «x» = C.M<bool>(«x»=«x»)', False),
    ('optional', 'type C = static member M(?x:int,?y:int) = x,y',
     'let caller «x» y = C.M(?«x»=«x»,y=y)', False),
    ('escaped', 'type C = static member M(``x.y``:int) = ``x.y``',
     'let caller «``x.y``» = C.M(«``x.y``»=«``x.y``»)', False),
    ('escapedUnderscore', 'type C = static member M(``_``:int) = ``_``',
     'let caller «``_``» = C.M(«``_``»=«``_``»)', False),
    ('tuple', 'type C = static member M(y:int,x:int) = x',
     'let caller «x» = C.M(0,«x»=«x»)', False),
    ('dotIndexer', GETTER, 'let caller (c:C) «x» = c.[«x»=«x»]', False),
    ('dotlessIndexer', GETTER, 'let caller (c:C) «x» = c[«x»=«x»]', False),
    ('unionCase', 'type T = One of x:int', 'let caller «x» = One(«x»=«x»)', False),
    ('initProperty', 'type C() = member val x=0 with get,set',
     'let caller «x» = C(x=«x»)', False),
    ('inherits', 'type B(x:int) = member _.P=x', 'type D(«x»:int) = inherit B(«x»=«x»)', False),
    ('objectExpr', '[<AbstractClass>]\ntype B(x:int) = abstract P:int',
     'let caller «x» = { new B(«x»=«x») with member _.P=«x» }', False),
    ('moduleThenType', 'module C =\n    let M b=b\ntype C = static member M(x:int)=x',
     'let caller x=C.«M»(x=x)', False),
    ('openedThenType', 'module Other =\n    module C =\n        let M b=b\nopen Other\n'
     'type C = static member M(x:int)=x', 'let caller x=C.«M»(x=x)', False),
    ('autoThenType', '[<AutoOpen>]\nmodule Other =\n    module C =\n        let M b=b\n'
     'type C = static member M(x:int)=x', 'let caller x=C.«M»(x=x)', False),
    ('overloaded', 'type C =\n    static member M(x:int)=x\n    static member M(x:bool)=0',
     'let caller («x»:int) = C.M(«x»=«x»)', False),
    ('autoStatic', 'type C = static member M(x:int)=x',
     'let caller «x» = C.M(«x»=«x»)', False),
    ('autoQualifier', 'module A =\n    let g a=a', 'let caller «x» = A.g(«x»=«x»)', False),
    ('openedQualifier', 'open ImportedFromControl\nmodule A =\n    let g a=a',
     'let caller «x» = A.g(«x»=«x»)', False),
    ('shadowCtor', 'let g b=b\nmodule B =\n    type g(x:int) = member _.P=x\nopen B',
     'let caller «x» = g(«x»=«x»)', False),
    ('nested', STATIC, 'let caller = let inner «x» = C.M(«x»=«x») in inner true', False),
    ('callbackArith', 'type C = static member M(x:int) = x',
     'let caller = List.map (fun «x» -> C.M(«x»=«x»+1)) [1]', False),
    ('callbackApply', STATIC, 'let caller = List.map (fun «x» -> C.M(«x»=not «x»)) [true]', False),
    ('callbackLocal', '', 'let caller = List.map (fun «x» -> «x»+«x») [1]', True),
    ('invalidNamed', STATIC, 'let caller () = C.M(x="bad")', False),
]
SIGNATURES = ['fsi', 'FsI', 'FSI']


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def make_file(path, marked, name, files):
    queries = []
    clean_lines = []
    for line, text in enumerate(marked.splitlines()):
        prefix = ''
        end = 0
        for match in re.finditer(r'«([^«»]+)»', text):
            prefix += text[end:match.start()]
            queries.append({'label': str(len(queries)), 'line': line,
                            'character': len(prefix.encode('utf-16-le')) // 2,
                            'token': match[1]})
            prefix += match[1]
            end = match.end()
        clean_lines.append(prefix + text[end:])
    path.write_text('\n'.join(clean_lines) + '\n')
    return {'case': name + '/' + path.name, 'path': str(path),
            'projectFiles': list(map(str, files)), 'queries': queries}


def prepare(out):
    manifest = []
    required = set()
    valid_files = []
    cases = [(name, head + '\n' + body, None, positive) for name, head, body, positive in CASES]
    for suffix in SIGNATURES:
        cases.append(('signature' + suffix, 'let ordinary «x»=«x»+«x»', suffix, False))
    cases.append(('invalidSignature', 'let ordinary («x»:int) = «x»+«x»', 'fsi', False))
    # This differently named .FsI is admitted by SDK400's command-line compiler.
    # The project FCS API reports the real int/string conformance mismatch.
    cases.append(('signatureConformance', 'let ordinary («x»:int) = «x»+«x»', 'FsI', False))
    for name, body, suffix, positive in cases:
        root = out / 'fixtures' / name
        root.mkdir(parents=True)
        # Use distinct physical basenames (including for .fsi pairs). Unique module names
        # let one real SDK project validate the positive fixture batch in compile order.
        implementation = root / ('Implementation' + name + '.fs')
        files = [implementation]
        if name == 'openedQualifier':
            prior = root / 'ImportedFromControl.fs'
            prior.write_text('module ImportedFromControl\ntype Proxy() = member _.g(x:int)=x\nlet A=Proxy()\n')
            files.insert(0, prior)
        header = 'module Probe' + name + '\n'
        if name == 'autoQualifier':
            prior = root / 'AutoImported.fs'
            prior.write_text('namespace ControlAuto\n[<AutoOpen>]\nmodule Imported =\n'
                             '    type Proxy()=member _.g(x:int)=x\n    let A=Proxy()\n')
            files.insert(0, prior)
            header = 'module ControlAuto.Consumer\n'
        if suffix:
            signature = root / ('Other' + name + '.' + suffix)
            files.insert(0, signature)
            result = {'invalidSignature': 'UnboundResultType', 'signatureConformance': 'string'}.get(name, 'int')
            manifest.append(make_file(signature, header + f'val ordinary : «x»:int -> {result}',
                                      name, files))
        if name == 'autoStatic':
            prior = root / 'AutoTerm.fs'
            prior.write_text('namespace ControlReceiver\n[<AutoOpen>]\nmodule Imported =\n'
                             '    type Proxy()=member _.M(x:int)=x\n    let C=Proxy()\n')
            files.insert(0, prior)
            header = 'module ControlReceiver.Consumer\n'
        manifest.append(make_file(implementation, header + body, name, files))
        project(root / 'Control.fsproj', ''.join(f'<Compile Include="{p.name}"/>' for p in files))
        if not name.startswith('invalid') and name != 'signatureConformance':
            valid_files.extend(files)
        if positive:
            required.add(name)
    build = out / 'sdk' / 'Valid.fsproj'
    project(build, ''.join(f'<Compile Include="../{p.relative_to(out).as_posix()}"/>'
                           for p in valid_files))
    (out / 'global.json').write_bytes((ROOT / 'global.json').read_bytes())
    return manifest, required, build


def execute(command, cwd, env, log):
    proc = subprocess.run(list(map(str, command)), cwd=cwd, env=env, capture_output=True,
                          text=True, timeout=240)
    log.write_text(proc.stdout + proc.stderr)
    return proc


def compiler(out, dotnet, manifest, build):
    env = {**os.environ, 'DOTNET_CLI_TELEMETRY_OPTOUT': '1', 'DOTNET_NOLOGO': '1',
           'DOTNET_CLI_USE_MSBUILD_SERVER': '0', 'MSBUILDDISABLENODEREUSE': '1'}
    checks = {'valid': build, 'invalidSignature': out / 'fixtures/invalidSignature/Control.fsproj',
              'invalidNamed': out / 'fixtures/invalidNamed/Control.fsproj',
              'signatureConformance': out / 'fixtures/signatureConformance/Control.fsproj'}
    for name, path in checks.items():
        p = execute([dotnet, 'build', path, '--nologo', '-v:q'], out, env, out / (name + '.log'))
        if (p.returncode == 0) != (name in {'valid', 'signatureConformance'}):
            raise RuntimeError(f'Unexpected SDK build result for {name}. See {out / (name + ".log")}')
    manifest_path = out / 'manifest.json'
    manifest_path.write_text(json.dumps(manifest, indent=2))
    p = execute([dotnet, 'fsi', '--exec', ORACLE, manifest_path], out, env, out / 'fcs.log')
    if p.returncode:
        raise RuntimeError(f'Compiler oracle failed. See {out / "fcs.log"}')
    checked = {r['case']: r for line in p.stdout.splitlines() if line.startswith('{')
               for r in [json.loads(line)]}
    for item in manifest:
        row = checked[item['case']]
        if row.get('valid') != (not item['case'].startswith(('invalid', 'signatureConformance/'))):
            raise RuntimeError(f'Unexpected FCS validity for {item["case"]}. See {out / "fcs.log"}')
    return checked


def compiler_range(r):
    return Path(r['path']), r['startLine'], r['startCharacter'], r['endLine'], r['endCharacter']


def actual_range(loc):
    uri = urlsplit(loc['uri'])
    start, end = loc['range']['start'], loc['range']['end']
    return (Path(unquote(uri.path)).as_posix() if uri.scheme == 'file' and not uri.netloc else loc['uri'], start['line'], start['character'], end['line'], end['character'])


def query(session, item, oracle, path, version, stage, required):
    results = []
    for position, symbol in zip(item['queries'], oracle['results'], strict=True):
        if symbol['label'] != position['label'] or not symbol.get('symbol'):
            raise RuntimeError(f'Missing FCS symbol for {item["case"]} at {position}')
        for method, flag in [('definition', None), ('references', True), ('references', False)]:
            params = {'path': str(path), 'version': version, 'line': position['line'],
                      'character': position['character']}
            if flag is None:
                expected = [compiler_range(symbol['symbol']['declaration'])]
            else:
                params['includeDeclaration'] = flag
                expected = sorted({compiler_range(o['range']) for o in symbol['projectReferences']
                                   if flag or not o['isDefinition']})
            # Dirty/reopened overlays keep the URI of the first source file.
            expected = sorted(((path if f == Path(item['path']) else f).as_posix(), *coordinates)
                              for f, *coordinates in expected)
            response = session.call('navigation/' + method, params)
            answer = response.get('result', {})
            actual = sorted(map(actual_range, answer.get('locations', [])))
            status = ('error' if 'error' in response else
                      'unavailable' if answer.get('completeness') != 'exact' else
                      'correct' if actual == expected else 'wrong')
            results.append({'case': item['case'], 'stage': stage, 'position': position, 'method': method,
                            'includeDeclaration': flag, 'source': answer.get('source'), 'status': status,
                            'required': required, 'expected': expected, 'actual': actual,
                            **({'error': response['error']} if 'error' in response else {})})
    return results


def prepare_receivers(out, helper, dotnet, manifest):
    env = {**os.environ, 'FSHARP_NAVIGATOR_DOTNET': str(dotnet)}
    artifacts = {}
    for item in manifest:
        fixture = Path(item['path']).parent
        if fixture.name not in {'named', 'qualified', 'overloaded', 'autoStatic', 'openedQualifier', 'autoQualifier'}:
            continue
        destination = out / 'prepared' / fixture.name
        destination.mkdir(parents=True)
        tags = ['--helper', helper, '--root', fixture, '--timeout-ms', '180000']
        model = destination / 'model.json'
        inventory = destination / 'environment.json'
        commands = [
            [dotnet, 'build', fixture / 'Control.fsproj', '--nologo', '-v:q'],
            [ROOT / 'cli/bin/run', 'projects', 'evaluate', *tags, '--output', model],
            [ROOT / 'cli/bin/run', 'libraries', 'environment', *tags, '--model', model, '--output', inventory],
        ]
        for i, command in enumerate(commands):
            execute(command, ROOT, env, destination / f'prepare-{i}.log').check_returncode()
        projects = json.loads(model.read_text())['projects']
        if not projects or not all(p.get('membershipReusable') for p in projects):
            raise RuntimeError(f'Helper could not certify project membership for {fixture}')
        artifacts[fixture.name] = {'evaluatedModelPath': str(model), 'builtinEnvironmentPath': str(inventory)}
    return artifacts


def replay(binary, out, manifest, checked, required, artifacts):
    rows = []
    for item in manifest:
        oracle = checked[item['case']]
        if not oracle['valid']:
            continue
        p = Path(item['path'])
        session = Native(binary)
        try:
            session.call('workspace/open', {'root': str(p.parent), 'mode': 'indexed',
                                            'cacheDir': str(out / 'cache' / p.parent.name)})
            session.call('document/open', {'path': str(p), 'text': p.read_text(), 'version': 1})
            rows.extend(query(session, item, oracle, p, 1, 'open', p.parent.name in required))
            assert session.call('workspace/stats', {})['result']['helperStarts'] == 0
        finally:
            session.close()

    for item in manifest:
        path = Path(item['path'])
        config = artifacts.get(path.parent.name)
        if config is None:
            continue
        session = Native(binary)
        try:
            session.call('workspace/open', {'root': str(path.parent), 'mode': 'indexed',
                                            'cacheDir': str(out / 'prepared-cache' / path.parent.name), **config})
            session.call('document/open', {'path': str(path), 'text': path.read_text(), 'version': 1})
            prepared = query(session, item, checked[item['case']], path, 1, 'prepared', False)
            for r in prepared:
                if path.parent.name == 'qualified':
                    r['required'] = True
                elif path.parent.name == 'named':
                    # The callee parameter can have named references beyond its body.
                    r['required'] = r['method'] == 'definition' or r['position']['label'] != '1'
            rows.extend(prepared)
            assert session.call('workspace/stats', {})['result']['helperStarts'] == 0
        finally:
            session.close()

    states = {Path(i['path']).parent.name: i for i in manifest}
    ordinary, named = states['ordinary'], states['named']
    p = Path(ordinary['path'])
    config = {'root': str(p.parent), 'mode': 'indexed', 'cacheDir': str(out / 'cache/ordinary')}
    for lifecycle in ['edit', 'restart']:
        session = Native(binary)
        try:
            session.call('workspace/open', config)
            session.call('document/open', {'path': str(p), 'text': p.read_text(), 'version': 1})
            rows.extend(query(session, ordinary, checked[ordinary['case']], p, 1, lifecycle, True))
            if lifecycle == 'edit':
                dirty = p.read_text().replace('= g(x=x)', '= C.M(x=x)')
                assert dirty.splitlines()[1:] == Path(named['path']).read_text().splitlines()[1:]
                session.call('document/change', {'path': str(p), 'text': dirty, 'version': 2})
                rows.extend(query(session, named, checked[named['case']], p, 2, 'dirty', False))
                stale = session.call('document/change', {'path': str(p), 'text': p.read_text(), 'version': 1})
                assert 'error' in stale
                rows.extend(query(session, named, checked[named['case']], p, 2, 'after-stale', False))
                session.call('document/close', {'path': str(p)})
                session.call('document/open', {'path': str(p), 'text': p.read_text(), 'version': 1})
                rows.extend(query(session, ordinary, checked[ordinary['case']], p, 1, 'reopen', True))
            assert session.call('workspace/stats', {})['result']['helperStarts'] == 0
        finally:
            session.close()
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--previous-binary', type=Path, help='Run first and retain its SQLite caches')
    parser.add_argument('--output-dir', required=True, type=Path, help='New directory for controlled inputs and evidence')
    parser.add_argument('--dotnet', type=Path, default=Path(shutil.which('dotnet') or ROOT / '.tools/dotnet/dotnet'))
    parser.add_argument('--helper', type=Path, help='Explicit helper for genuine project and metadata preparation')
    parser.add_argument('--require-full-coverage', action='store_true',
                        help='Fail on every unavailable operation in addition to the required positives')
    args = parser.parse_args()
    out = args.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=False)
    manifest, required, build = prepare(out)
    sdk = subprocess.check_output([args.dotnet.resolve(), '--version'], cwd=out, text=True).strip()
    checked = compiler(out, args.dotnet.resolve(), manifest, build)
    if args.helper:
        # Match the unmodified CLI environment that the helper actually certifies.
        smoke.ENV = {**os.environ, 'FSHARP_NAVIGATOR_DOTNET': str(args.dotnet.resolve())}
    artifacts = prepare_receivers(out, args.helper.resolve(), args.dotnet.resolve(), manifest) if args.helper else {}
    report = {'helperSha256': digest(args.helper) if args.helper else None,
              'preparedInputs': {case: {k: digest(Path(v)) for k, v in paths.items()}
                                 for case, paths in artifacts.items()},
              'sdk': sdk, 'oracleSha256': digest(ORACLE), 'oracle': 'SDK build and full project FCS',
              'comparison': 'FCS declaration range and complete project uses with explicit declaration flag',
              'sdkBuilds': {'valid': True, 'invalidNamed': False, 'invalidSignature': False,
                            'signatureConformance': True},
              'fcsInvalidControls': [case for case, r in checked.items() if not r['valid']],
              'sources': {str(path.relative_to(out)): digest(path)
                          for path in sorted({Path(f) for i in manifest for f in i['projectFiles']})},
              'runs': {}}
    for label, binary in [('previous', args.previous_binary), ('candidate', args.binary)]:
        if not binary:
            continue
        rows = replay(binary.resolve(), out, manifest, checked, required, artifacts)
        counts = dict(Counter(r['status'] for r in rows))
        qualified = Counter(r['status'] for r in rows if r['case'].startswith('qualified/') and r['stage'] == 'open')
        report['runs'][label] = {'binarySha256': digest(binary), 'counts': counts,
                                 'unpreparedQualifiedEquality': dict(qualified), 'queries': rows}
        print(label, len(rows), counts, 'unprepared qualified equality', dict(qualified), flush=True)
        (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return int(any(r['status'] in {'error', 'wrong'} or ((r['required'] or args.require_full_coverage) and r['status'] != 'correct')
                   for r in report['runs']['candidate']['queries']))


if __name__ == '__main__':
    sys.exit(main())
