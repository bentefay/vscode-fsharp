#!/usr/bin/env python3
"""Reproducible JSONL correctness/performance runner; Python standard library only.

The measured program is the Rust daemon and every descendant it starts. The
controller is deliberately outside the measured process tree. No missing answer,
approximate answer or unvalidated corpus query can qualify as a performance win.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import queue
import re
import shlex
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from urllib.parse import unquote, urlparse

ROOT = Path(__file__).resolve().parents[1]
MARKER = re.compile(r"\(\*@([A-Za-z0-9_]+)\*\)")
TOKEN = re.compile(r"``[^`]+``|[^\W\d][\w']*")


def utf16(text):
    return len(text.encode("utf-16-le")) // 2


def strip_markers(text, relative):
    chunks, marks, offset = [], {}, 0
    for match in MARKER.finditer(text):
        chunks.append(text[offset:match.start()])
        prefix = "".join(chunks)
        token_match = TOKEN.match(text, match.end())
        token = token_match[0] if token_match else text[match.end():match.end()+1]
        if not token or token.isspace():
            raise ValueError(f"Marker {match[1]} has no following token")
        line = prefix.count("\n")
        col = utf16(prefix.rsplit("\n", 1)[-1])
        marks[match[1]] = {"file": relative, "line": line, "character": col,
                           "endCharacter": col + utf16(token), "token": token}
        offset = match.end()
    chunks.append(text[offset:])
    return "".join(chunks), marks


def materialize(destination, source_root=None):
    source = Path(source_root) if source_root else ROOT / "fixtures" / "sources"
    destination.mkdir(parents=True, exist_ok=True)
    markers = {}
    for item in sorted(source.rglob("*")):
        if not item.is_file():
            continue
        relative = item.relative_to(source).as_posix()
        text, local = strip_markers(item.read_bytes().decode("utf-8"), relative)
        duplicates = set(markers) & set(local)
        if duplicates:
            raise ValueError(f"Duplicate markers: {duplicates}")
        markers.update(local)
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(text.encode("utf-8"))
    return markers


def digest_files(root):
    digest = hashlib.sha256()
    for path in sorted(root.rglob("*")):
        if path.is_file() and not any(part in {"obj", "bin"} for part in path.parts):
            name = path.relative_to(root).as_posix().encode()
            data = path.read_bytes()
            digest.update(len(name).to_bytes(8, "little") + name)
            digest.update(len(data).to_bytes(8, "little") + data)
    return digest.hexdigest()


def file_sha(path):
    path = Path(path)
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


def source_sha():
    paths = []
    for directory in ('crates', 'semantic', 'extension/src'):
        paths += [p for p in (ROOT / directory).rglob('*') if p.is_file() and p.suffix in {'.rs','.fs','.fsi','.ts','.toml','.fsproj'}
                  and not any(part in {'bin','obj','node_modules','target'} for part in p.relative_to(ROOT).parts)]
    digest = hashlib.sha256()
    for path in sorted(paths):
        digest.update(path.relative_to(ROOT).as_posix().encode() + b'\0' + path.read_bytes() + b'\0')
    return digest.hexdigest()


def snapshot_engine(args, work):
    original = shlex.split(args.engine)
    command = original[:]
    executable = Path(command[0]).resolve() if Path(command[0]).is_file() else Path(shutil.which(command[0]) or command[0])
    binary_hash = file_sha(executable)
    if args.engine_sha and binary_hash!=args.engine_sha:
        raise RuntimeError(f'Engine SHA differs from frozen candidate: expected {args.engine_sha}, got {binary_hash}')
    if executable.name == 'fsharp-navigator' and binary_hash:
        destination = work / 'bin' / ('fsharp-navigator-' + binary_hash[:16])
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(executable, destination)
        if file_sha(destination) != binary_hash:
            raise RuntimeError('Engine changed during snapshot copy; rerun after build completes')
        command[0] = str(destination)
        args.engine = shlex.join(command)
    adapter_hashes={}
    for index,item in enumerate(command[1:],1):
        if Path(item).name=='fsac_adapter.py' and Path(item).is_file():
            original_adapter=Path(item).resolve();adapter_hash=file_sha(original_adapter)
            destination=work/'bin'/('fsac-adapter-'+adapter_hash[:16]+'.py')
            destination.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(original_adapter,destination)
            if file_sha(destination)!=adapter_hash:raise RuntimeError('FSAC adapter changed during snapshot')
            adapter_hashes[str(original_adapter)]=adapter_hash;command[index]=str(destination)
    args.engine=shlex.join(command)
    git = subprocess.run(['git','-C',str(ROOT),'rev-parse','HEAD'],capture_output=True,text=True)
    status = subprocess.run(['git','-C',str(ROOT),'status','--porcelain'],capture_output=True,text=True)
    helper = shlex.split(args.helper) if args.helper else original
    helper_dlls = {str(Path(item).resolve()):file_sha(item) for item in helper if item.endswith('.dll')}
    prepared_hashes={name:file_sha(path) for name,path in [('model',args.evaluated_model),('catalog',args.library_catalog),('builtin_environment',args.builtin_environment),('semantic_snapshot',args.semantic_snapshot)] if path}
    return {'original_command':original, 'executed_command':command, 'engine_binary_sha256':binary_hash,
            'source_git_commit':git.stdout.strip() if git.returncode==0 else None,
            'source_git_dirty':bool(status.stdout.strip()) if status.returncode==0 else None,
            'source_sha256_before':source_sha(), 'helper_dll_sha256':helper_dlls,
            'adapter_sha256':adapter_hashes,
            'decompiler_file_sha256':{str(Path(item).resolve()):file_sha(item) for item in shlex.split(args.decompiler or '') if Path(item).is_file()},
            'measurement_script_sha256':file_sha(__file__),
            'fixture_manifest_sha256':file_sha(args.manifest),
            'prepared_input_sha256':prepared_hashes,
            'dependency_locks':{name:file_sha(ROOT/name) for name in ('Cargo.lock','package-lock.json','flake.lock','global.json')},
            'build_mode':'release' if '/release/' in str(executable) else 'external-or-unspecified',
            'binary_snapshotted':command!=original}


def process_identity(pid):
    fields=Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()
    return int(fields[1]),int(fields[19])


def process_tree_details(pid, expected_starttime=None, retained=None):
    """Read PPids: this sandbox exposes empty task/children for real children."""
    parents,identities = {},{}
    for path in Path('/proc').iterdir():
        if not path.name.isdigit():continue
        try:
            parent,starttime=process_identity(int(path.name))
            parents[int(path.name)]=parent
            identities[int(path.name)]=starttime
        except (FileNotFoundError,ProcessLookupError,PermissionError,IndexError):pass
    if expected_starttime is not None and identities.get(pid)!=expected_starttime and not retained:
        return 0,[],{},{},{}
    children={}
    for child,parent in parents.items():children.setdefault(parent,[]).append(child)
    roots=[pid] if expected_starttime is None or identities.get(pid)==expected_starttime else []
    for owned,starttime in list((retained or {}).items()):
        if identities.get(owned)==starttime:roots.append(owned)
        else:retained.pop(owned,None)
    todo, seen, total, residents = roots, set(), 0, {}
    page_kib = os.sysconf("SC_PAGE_SIZE") // 1024
    while todo:
        current = todo.pop()
        if current in seen:
            continue
        seen.add(current)
        try:
            resident=int(Path(f"/proc/{current}/statm").read_text().split()[1]) * page_kib
            if process_identity(current)!=(parents.get(current),identities.get(current)):
                continue
            residents[current]=resident
            if retained is not None:retained[current]=identities[current]
            total += residents[current]
            todo.extend(children.get(current,[]))
        except (FileNotFoundError, ProcessLookupError, PermissionError, IndexError):
            pass
    return total, sorted(residents), residents, parents, identities


def process_tree_rss(pid):
    total,pids,_,_,_=process_tree_details(pid)
    return total,pids


def subtree_rss(pid, residents, parents, identities=None, expected_starttime=None):
    if expected_starttime is not None and (identities or {}).get(pid)!=expected_starttime:return 0
    selected={pid}
    while True:
        expanded=selected | {child for child,parent in parents.items() if parent in selected}
        if expanded==selected:return sum(residents.get(child,0) for child in selected)
        selected=expanded


class Session:
    def __init__(self, command, timeout=60, sample_interval=.01, memory_limit_mib=4096):
        self.started = time.perf_counter()
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, text=True, bufsize=1,
                                        start_new_session=True)
        self.timeout, self.next_id = timeout, 0
        self.failed = False
        self.output, self.stderr, self.samples = queue.Queue(), [], []
        self.detailed_samples=[]
        self.backend_pid=None
        self.backend_starttime=None
        self.root_starttime=process_identity(self.process.pid)[1]
        self.owned_identities={self.process.pid:self.root_starttime}
        self.sampler_cpu_seconds=0.0
        self.sampler_wall_seconds=0.0
        self.include_declaration=False
        self.reference_normalization='strict'
        self.open_documents={}
        self.stop_event = threading.Event()
        self.sample_interval = sample_interval
        self.memory_limit_kib = memory_limit_mib * 1024
        threading.Thread(target=self._read_stdout, daemon=True).start()
        threading.Thread(target=self._read_stderr, daemon=True).start()
        self.sampler = threading.Thread(target=self._sample, daemon=True)
        self.sampler.start()

    def _read_stdout(self):
        for line in self.process.stdout:
            try:
                self.output.put(json.loads(line))
            except json.JSONDecodeError:
                self.output.put({"error": {"message": "Non-JSON stdout: " + line[:500]}})
        self.output.put({"error": {"message": "Daemon exited before responding"}})

    def _read_stderr(self):
        for line in self.process.stderr:
            self.stderr.append(line.rstrip())
            self.stderr[:] = self.stderr[-200:]

    def _sample(self):
        while not self.stop_event.is_set():
            sample_cpu=time.thread_time();sample_wall=time.perf_counter()
            rss, pids, residents, parents, identities = process_tree_details(self.process.pid,self.root_starttime,self.owned_identities)
            sampled=time.perf_counter()
            self.samples.append((sampled, rss, pids))
            self.detailed_samples.append((sampled,residents,parents,identities))
            self.sampler_cpu_seconds+=time.thread_time()-sample_cpu
            self.sampler_wall_seconds+=time.perf_counter()-sample_wall
            if self.memory_limit_kib and rss > self.memory_limit_kib:
                import signal
                self.failed = True
                self.output.put({"error": {"message": f"Memory guard aborted process tree at {rss} KiB"}})
                try:
                    os.killpg(self.process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                break
            self.stop_event.wait(self.sample_interval)

    def call(self, method, params=None):
        self.next_id += 1
        started = time.perf_counter()
        sampler_cpu_before=self.sampler_cpu_seconds
        initial_rss, _ = process_tree_rss(self.process.pid)
        try:
            if self.failed:
                raise BrokenPipeError("Previous request failed")
            self.process.stdin.write(json.dumps({"id": self.next_id, "method": method,
                                                "params": params or {}}) + "\n")
            self.process.stdin.flush()
            deadline = started + self.timeout
            while True:
                response = self.output.get(timeout=max(.001, deadline - time.perf_counter()))
                if response.get("id") in (None, self.next_id):
                    break
                if time.perf_counter() >= deadline:
                    raise queue.Empty
        except (queue.Empty, BrokenPipeError, OSError) as exc:
            self.failed = True
            response = {"error": {"message": f"{type(exc).__name__}: request failed or timed out"}}
        ended = time.perf_counter()
        result=response.get('result',{})
        if 'error' not in response and params and method in ('document/open','document/change','document/close'):
            path=str(Path(params['path']).resolve())
            if method=='document/close':self.open_documents.pop(path,None)
            else:self.open_documents[path]=params.get('version')
        if isinstance(result,dict) and result.get('backendPid'):
            self.backend_pid=result['backendPid']
        rss, pids, residents, parents, identities = process_tree_details(self.process.pid,self.root_starttime,self.owned_identities)
        if self.backend_pid and self.backend_starttime is None:self.backend_starttime=identities.get(self.backend_pid)
        samples = [value for when, value, _ in self.samples if started <= when <= ended]
        measurement = {"latency_ms": (ended - started) * 1000,
                       "peak_tree_rss_kib": max([initial_rss, rss] + samples),
                       "steady_tree_rss_kib": rss, "owned_pids": pids,
                       "sampler_cpu_ms_during_request":(self.sampler_cpu_seconds-sampler_cpu_before)*1000}
        if self.backend_pid:
            # This adapter has exactly one launched backend. Retain its observed
            # descendants even when MSBuild reparents them; subtract only bridge.
            backend_samples=[sum(rs.values())-rs.get(self.process.pid,0) for when,rs,ps,ids in self.detailed_samples if started<=when<=ended]
            backend_steady=sum(residents.values())-residents.get(self.process.pid,0)
            measurement.update(backend_peak_tree_rss_kib=max([backend_steady]+backend_samples),backend_steady_tree_rss_kib=backend_steady,
                               bridge_steady_rss_kib=residents.get(self.process.pid,0),backend_pid=self.backend_pid,backend_starttime_ticks=self.backend_starttime)
            measurement['bridge_peak_rss_kib']=max([residents.get(self.process.pid,0)]+[rs.get(self.process.pid,0) for when,rs,_,_ in self.detailed_samples if started<=when<=ended])
        return response, measurement

    def stats(self):
        response, _ = self.call("workspace/stats")
        return response.get("result", {})

    def close(self):
        if self.process.poll() is None and not self.failed:
            self.call("shutdown")
        try:
            self.process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            # Kill this owned process group, including a stalled helper.
            import signal
            os.killpg(self.process.pid, signal.SIGKILL)
            self.process.wait()
        self.stop_event.set()
        self.sampler.join(timeout=1)
        import signal
        for pid,starttime in list(self.owned_identities.items()):
            if pid==self.process.pid:continue
            try:
                if process_identity(pid)[1]==starttime:os.kill(pid,signal.SIGKILL)
            except (FileNotFoundError,ProcessLookupError,PermissionError):pass


def canonical_location(location, root):
    uri = location.get("uri", location.get("path", ""))
    parsed = urlparse(uri)
    path = Path(unquote(parsed.path) if parsed.scheme == "file" else uri).resolve()
    try:
        relative = path.relative_to(root.resolve()).as_posix()
    except ValueError:
        relative = str(path)
    start = location.get("range", {}).get("start", {})
    end = location.get("range", {}).get("end", {})
    return (relative, start.get("line"), start.get("character"), end.get("line"), end.get("character"))


def expected_location(marker):
    return (marker["file"], marker["line"], marker["character"], marker["line"], marker["endCharacter"])


def assess(spec, response, markers, root):
    if "error" in response:
        return {"correct": False, "reason": "protocol_error", "details": response["error"]}
    result = response.get("result", {})
    if spec.get("expectNotExact"):
        return {"correct": result.get("completeness") != "exact" or not result.get("locations"),
                "reason": "stale_resolution_guard"}
    if result.get("completeness") != "exact":
        return {"correct": False, "reason": "not_exact", "completeness": result.get("completeness")}
    if spec["method"] == "hover":
        hover = result.get("hover")
        correct = bool(hover) == spec["expectHover"]
        if hover and spec.get("contains"):
            correct = correct and spec["contains"] in hover.get("markdown", "")
        if spec.get('expectGenericArguments'):
            markdown=hover.get('markdown','') if hover else ''
            actual=result.get('genericArguments') or (hover or {}).get('genericArguments')
            if not isinstance(actual,dict):
                actual=dict(re.findall(r"^\s*\*\s*`('?[A-Za-z][A-Za-z0-9_]*)`\s+is\s+`([^`]+)`",markdown,re.M))
            normalized={name.lstrip("'"):value for name,value in actual.items()}
            correct=correct and all(normalized.get(name.lstrip("'"))==value for name,value in spec['expectGenericArguments'].items())
        if spec.get('expectPropertyResult'):
            expected=spec['expectPropertyResult'];signature=((hover or {}).get('markdown','')).split('\n\n')[0]
            pattern=r'\b'+re.escape(expected['name'])+r'\s*:\s*(?:[A-Za-z_][A-Za-z0-9_]*\.)*'+re.escape(expected['type'])+r'\b'
            correct=correct and re.search(pattern,signature) is not None
        return {"correct": correct, "reason": "hover_match" if correct else "hover_mismatch"}
    actual_list = [canonical_location(x, root) for x in result.get("locations", [])]
    actual = set(actual_list)
    alternatives = spec.get("expectedAny", [spec.get("expected", [])])
    def expected_span(item):
        if isinstance(item,str):
            return expected_location(markers[item])
        start,end=markers[item['start']],markers[item['end']]
        if start['file']!=end['file']:
            raise ValueError('Expected span crosses files')
        return (start['file'],start['line'],start['character'],end['line'],end['endCharacter'])
    def item_options(item):
        if isinstance(item,dict) and 'at' in item:
            return {expected_span(item['at'])} | {expected_span({'start':start,'end':item['at']}) for start in item.get('qualifiedFrom',[])}
        return {expected_span(item)}
    expected = [[item_options(item) for item in group] for group in alternatives]
    def exact_assignment(groups):
        if len(groups)!=len(actual):return False
        assigned={}
        def match(index,seen):
            for location in groups[index]&actual:
                if location in seen:continue
                seen.add(location)
                if location not in assigned or match(assigned[location],seen):
                    assigned[location]=index;return True
            return False
        return all(match(index,set()) for index in range(len(groups)))
    duplicates=len(actual_list)-len(actual)
    correct = any(exact_assignment(group) for group in expected) and (duplicates==0 or (spec['method']=='references' and spec.get('referenceNormalization')=='vscode'))
    return {"correct": correct, "reason": "locations_match" if correct else "locations_mismatch",
            "actual": sorted(actual), "raw_location_count":len(actual_list), "distinct_location_count":len(actual), "duplicate_location_count":duplicates,
            "reference_normalization":spec.get('referenceNormalization','strict'),
            "expected_any": [[sorted(options) for options in group] for group in expected]}


def numeric_delta(before, after):
    result = {}
    for key, value in after.items():
        if isinstance(value, (int, float)) and not isinstance(value, bool):
            previous = before.get(key)
            if isinstance(previous, (int, float)) and not isinstance(previous, bool):
                result[key] = value - previous
    return result


def query(session, spec, markers, workspace, phase, mode, repetition, rows, validate=True):
    spec={**spec,'referenceNormalization':session.reference_normalization}
    if session.include_declaration and spec['method']=='references':
        declaration=spec.get('declaration',spec['at'] if spec['at'].endswith('_def') else None)
        if not declaration:
            raise ValueError('Declaration-inclusive benchmark requires explicit declaration cursor marker')
        spec=copy.deepcopy(spec)
        if 'expectedAny' in spec:
            for alternative in spec['expectedAny']:alternative.append(declaration)
        else:spec['expected'].append(declaration)
    marker = markers[spec["at"]]
    # A user requests cursor providers in an opened editor document. FSAC does
    # not promise a foreground check for arbitrary unopened project files.
    # Apply this lifecycle to both engines and retain its separate open cost.
    if str((workspace/marker['file']).resolve()) not in session.open_documents:
        open_document(session,marker,workspace,mode,repetition,rows)
        rows[-1]['trigger']='first_cursor_query_in_document'
    before = session.stats()
    request_params = {
        "path": str(workspace / marker["file"]), "line": marker["line"],
        "character": marker["character"] + min(1, max(0,len(marker.get("token", ""))-1)),
        "includeDeclaration": session.include_declaration}
    if spec.get("projectPath"):request_params["projectPath"]=str(workspace/spec["projectPath"])
    response, measurement = session.call("navigation/" + spec["method"], request_params)
    since_process_start_ms = (time.perf_counter() - session.started) * 1000
    after = session.stats()
    check = assess(spec, response, markers, workspace) if validate else {"correct": None, "reason": "no_semantic_oracle"}
    row = {"phase": phase, "mode": mode, "repetition": repetition, "query": spec["name"],
           "feature": spec.get("feature"), **measurement, **check,
           "since_process_start_ms": since_process_start_ms,
           "completeness": response.get("result", {}).get("completeness"),
           "request": {"method":"navigation/"+spec['method'],"params":request_params},
           "counter_delta": numeric_delta(before, after), "response": response}
    rows.append(row)
    return row


def open_document(session, marker, workspace, mode, repetition, rows):
    before=session.stats()
    path=workspace/marker['file']
    response,measurement=session.call('document/open',{'path':str(path),'text':path.read_bytes().decode('utf-8',errors='replace'),'version':1})
    delta=numeric_delta(before,session.stats())
    rows.append({'phase':'document_open','mode':mode,'repetition':repetition,'query':marker['file'],
                 **measurement,'counter_delta':delta,'response':response,'correct':None,
                 'zero_ast_work':delta.get('filesParsed')==0})


def open_workspace(args, workspace, mode, cache, rows, repetition, phase):
    session = Session(shlex.split(args.engine), args.timeout, args.sample_interval, args.memory_limit_mib)
    session.include_declaration=args.include_declaration
    session.reference_normalization=args.reference_normalization
    params = {"root": str(workspace), "mode": mode, "cacheDir": str(cache)}
    if args.helper:
        helper = shlex.split(args.helper)
        params.update(helperPath=helper[0], helperArgs=helper[1:])
    if args.evaluated_model:
        params["evaluatedModelPath"] = str(args.evaluated_model.resolve())
    if args.library_catalog:
        params["metadataPath"] = str(args.library_catalog.resolve())
    if args.builtin_environment:params["builtinEnvironmentPath"]=str(args.builtin_environment.resolve())
    if args.decompiler:
        decompiler=shlex.split(args.decompiler);params.update(decompilerPath=decompiler[0],decompilerArgs=decompiler[1:])
    if args.semantic_snapshot:
        params["semanticSnapshotPath"] = str(args.semantic_snapshot.resolve())
    response, measurement = session.call("workspace/open", params)
    measurement["latency_ms"] = (time.perf_counter() - session.started) * 1000
    if session.samples:
        measurement["peak_tree_rss_kib"] = max(measurement["peak_tree_rss_kib"], max(s[1] for s in session.samples))
    stats=response.get('result',{})
    startup_gates={key:stats.get(key)==0 for key in ('filesRead','bytesRead','filesParsed','compilerLaunches')}
    rows.append({"phase": phase, "mode": mode, "repetition": repetition, "query": "workspace/open",
                 "startup_zero_source_work_gates":startup_gates,
                 **measurement, "correct": None, "reason": "setup_only", "response": response})
    return session


def prepare_edit(edit, markers, workspace, source_root=None):
    """Recalculate marker positions from annotated source after a multi-file edit.

    Marker-specific substitutions prevent a rename of one record field from
    accidentally renaming a same-spelled field in another type. No source file
    is changed on disk: the returned texts are document overlays.
    """
    changed, updated = [], dict(markers)
    changes = edit.get('changes', [edit])
    for change in changes:
        relative = change['file']
        if 'markedReplacements' in change or 'appendMarkedText' in change:
            source = ((Path(source_root) if source_root else ROOT / 'fixtures' / 'sources') / relative).read_bytes().decode('utf-8')
            for replacement in change.get('markedReplacements', []):
                old, new = replacement['old'], replacement['new']
                expected_count = replacement.get('count', 1)
                if source.count(old) != expected_count:
                    raise ValueError(f"Edit anchor count differs: {edit['name']} {old!r}")
                source = source.replace(old, new)
            source += change.get('appendMarkedText', '')
            text, local = strip_markers(source, relative)
            updated = {name: marker for name, marker in updated.items() if marker['file'] != relative}
            updated.update(local)
        else:
            source = (workspace / relative).read_text()
            if change['old'] not in source:
                raise ValueError(f"Edit anchor absent: {edit['name']}")
            text = source.replace(change['old'], change['new'], 1)
        changed.append((workspace / relative, text))
    return changed, updated


def edit_checks(edit, queries):
    by_name = {item['name']: item for item in queries}
    checks = edit.get('checks', [{'query': edit.get('query'), **{key: edit[key] for key in ('expectNotExact',) if key in edit}}])
    result = []
    for check in checks:
        if check['query'] not in by_name:
            continue
        spec = copy.deepcopy(by_name[check['query']])
        spec.update({key: check[key] for key in ('expected', 'expectedAny', 'expectNotExact') if key in check})
        if 'removeExpected' in check or 'addExpected' in check:
            removed = set(check.get('removeExpected', []))
            spec['expected'] = [item for item in spec['expected'] if (item if isinstance(item, str) else item.get('at')) not in removed]
            spec['expected'].extend(check.get('addExpected', []))
        result.append(spec)
    return result


def run_edits(args, manifest, queries, session, markers, workspace, mode, repetition, rows):
    selected = set(args.edit_names.split(',')) if args.edit_names else None
    for edit_index, edit in enumerate(manifest.get('edits', [])):
        if selected is not None and edit['name'] not in selected:
            continue
        checks = edit_checks(edit, queries)
        if not checks:
            continue
        changes, edit_markers = prepare_edit(edit, markers, workspace, args.fixture_source)
        changes_before = session.stats()
        measurements, responses = [], []
        edit_started = time.perf_counter()
        try:
            for path, text in changes:
                response, measurement = session.call('document/change', {'path': str(path), 'text': text, 'version': 2 + edit_index * 2})
                responses.append(response)
                measurements.append(measurement)
            changes_after = session.stats()
            edit_ms = (time.perf_counter() - edit_started) * 1000
            for check_index, spec in enumerate(checks):
                row = query(session, spec, edit_markers, workspace, edit['name'], mode, repetition, rows)
                row['edit_ms'] = edit_ms
                row['edit_to_answer_ms'] = (time.perf_counter() - edit_started) * 1000
                row['query_only_ms'] = row['latency_ms']
                row['latency_ms'] = row['edit_to_answer_ms']
                row['edit_check_index'] = check_index
                row['edit_responses'] = responses
                row['edit_counter_delta'] = numeric_delta(changes_before, changes_after)
                row['changed_files'] = [str(path.relative_to(workspace)) for path, _ in changes]
                row['peak_tree_rss_kib'] = max([row['peak_tree_rss_kib']] + [item['peak_tree_rss_kib'] for item in measurements])
        finally:
            for path, _ in changes:
                if args.edit_reset=='change':
                    session.call('document/change',{'path':str(path),'text':path.read_bytes().decode('utf-8'),'version':3+edit_index*2})
                else:session.call('document/close', {'path': str(path)})


def run_fixtures(args, work, rows):
    workspace = work / "fixtures"
    markers = materialize(workspace,args.fixture_source)
    manifest = json.loads(args.manifest.read_text())
    queries = manifest["queries"]
    if args.only:
        wanted = set(args.only.split(','))
        queries = [item for item in queries if item['name'] in wanted]
        if not queries or wanted-set(item['name'] for item in queries):
            raise ValueError('Unknown or empty --only query selection')
    if args.first_query:
        first=next((item for item in queries if item['name']==args.first_query),None)
        if first is None:raise ValueError('--first-query must name a selected query')
        queries=[first]+[item for item in queries if item is not first]
    selected_edit_names = set(args.edit_names.split(',')) if args.edit_names else None
    all_edit_names = {edit['name'] for edit in manifest.get('edits', [])}
    if selected_edit_names is not None and selected_edit_names - all_edit_names:
        raise ValueError('Unknown --edit-names selection: ' + ', '.join(sorted(selected_edit_names - all_edit_names)))
    selected_edits = [edit for edit in manifest.get('edits', []) if selected_edit_names is None or edit['name'] in selected_edit_names]
    selected_edit_check_count = sum(len(edit_checks(edit, queries)) for edit in selected_edits)
    if args.edits_only and selected_edit_check_count == 0:
        raise ValueError('--edits-only selects no checks; refusing an empty evaluation')
    fingerprint = hashlib.sha256((digest_files(workspace) + json.dumps(manifest, sort_keys=True) + (';includeDeclaration=true' if args.include_declaration else '') + (';references=vscode' if args.reference_normalization=='vscode' else '') + ';editReset=' + args.edit_reset + ';documentPolicy=open-before-first-query-v2').encode()).hexdigest()
    for mode in args.modes.split(","):
        for repetition in range(args.repetitions):
            cache = work / "cache" / mode
            shutil.rmtree(cache, ignore_errors=True)
            session = open_workspace(args, workspace, mode, cache, rows, repetition, "cold_start")
            try:
                open_document(session,markers[queries[0]['at']],workspace,mode,repetition,rows)
                first = query(session, queries[0], markers, workspace, "first_query", mode, repetition, rows)
                first["cold_start_plus_query_ms"] = first["since_process_start_ms"]
                cold_answer={**first, "phase": "cold_start_to_first_answer", "latency_ms": first["since_process_start_ms"],
                             "peak_tree_rss_kib": max([first["peak_tree_rss_kib"]] + [sample[1] for sample in session.samples])}
                if session.backend_pid:
                    cold_answer['backend_peak_tree_rss_kib']=max([first['backend_peak_tree_rss_kib']]+[sum(rs.values())-rs.get(session.process.pid,0) for _,rs,_,_ in session.detailed_samples])
                rows.append(cold_answer)
                opened={markers[queries[0]['at']]['file']}
                for spec in queries:
                    marker=markers[spec['at']]
                    if marker['file'] not in opened and len(opened)<args.open_documents:
                        open_document(session,marker,workspace,mode,repetition,rows);opened.add(marker['file'])
                if args.build_index:
                    response,measurement=session.call('workspace/index')
                    rows.append({'phase':'explicit_index_completion','mode':mode,'repetition':repetition,
                                 **measurement,'response':response,'correct':None,
                                 'since_process_start_ms':(time.perf_counter()-session.started)*1000})
                if not args.edits_only:
                    for spec in queries:
                        query(session, spec, markers, workspace, "correctness", mode, repetition, rows)
                if args.correctness_only:
                    rows.append({"phase":"final_stats","mode":mode,"repetition":repetition,"stats":session.stats(),"correct":None})
                    continue
                for cycle in range(0 if args.edits_only else args.warm_repetitions):
                    # Rotate traversal: stress distinct files/symbols, not one memoized answer.
                    for spec in queries[cycle % len(queries):] + queries[:cycle % len(queries)]:
                        query(session, spec, markers, workspace, "warm_query", mode, repetition, rows)
                run_edits(args, manifest, queries, session, markers, workspace, mode, repetition, rows)
                rows.append({"phase": "final_stats", "mode": mode, "repetition": repetition,
                             "stats": session.stats(), "stderr": session.stderr[:], "correct": None})
            finally:
                session.close()
            if args.edits_only:
                continue
            restored = open_workspace(args, workspace, mode, cache, rows, repetition, "cache_restore_start")
            try:
                for file in dict.fromkeys(markers[spec["at"]]["file"] for spec in queries):
                    open_document(restored,{"file":file},workspace,mode,repetition,rows)
                    rows[-1]["phase"]="cache_restore_document_open"
                for spec in queries:
                    query(restored, spec, markers, workspace, "cache_restore_query", mode, repetition, rows)
            finally:
                restored.close()
    return {"kind": "fixture", "fingerprint": fingerprint, "files": sum(1 for path in workspace.rglob('*') if path.suffix in {'.fs','.fsi'} and not any(part in {'obj','bin'} for part in path.relative_to(workspace).parts)),
            "query_count": len(queries), "query_names":[spec['name'] for spec in queries],
            "edit_check_count":0 if args.correctness_only else selected_edit_check_count,
            "selected_edit_names":[] if args.correctness_only else [edit['name'] for edit in selected_edits if edit_checks(edit,queries)],
            "notes": manifest["notes"]}


def corpus_inventory(root, count):
    command = ["git", "-C", str(root), "ls-files", "-z"]
    listing = subprocess.run(command, capture_output=True, check=True).stdout.split(b"\0")
    files = [os.fsdecode(path) for path in listing if path.endswith((b".fs", b".fsi", b".fsx"))]
    files.sort()
    total_bytes = sum((root / name).stat().st_size for name in files if (root / name).is_file())
    sha = subprocess.run(["git", "-C", str(root), "rev-parse", "HEAD"], capture_output=True, check=True, text=True).stdout.strip()
    markers, queries = {}, []
    # Uniformly sample the sorted tracked file universe, then find an ordinary declaration.
    stride = max(1, len(files) // max(1, count))
    for name in files[::stride]:
        text = (root / name).read_text(errors="replace")
        match = re.search(r"(?m)^\s*let\s+(?:private\s+|internal\s+|mutable\s+|rec\s+|inline\s+)*([A-Za-z_][A-Za-z0-9_']*)", text)
        if not match:
            continue
        method = "references" if len(queries) % 2 else "definition"
        next_use = re.search(r"\b" + re.escape(match[1]) + r"\b", text[match.end(1):])
        start = match.end(1) + next_use.start() if method == "definition" and next_use else match.start(1)
        prefix = text[:start]
        key = f"corpus_{len(queries):04d}"
        markers[key] = {"file": name, "line": prefix.count("\n"), "character": utf16(prefix.rsplit("\n", 1)[-1]), "token": match[1]}
        queries.append({"name": key, "method": method, "at": key, "feature": "unvalidated_corpus_query"})
        if len(queries) >= count:
            break
    if not queries:
        raise ValueError("No sampleable declaration found")
    return markers, queries, {"kind": "corpus", "root": str(root), "sha": sha, "tracked_fsharp_files": len(files),
                             "tracked_fsharp_bytes": total_bytes, "fingerprint": sha,
                             "query_count": len(queries), "oracle": "unavailable; no correctness/speedup claim"}


def run_corpus(args, work, rows):
    root = Path(args.corpus).resolve()
    markers, queries, metadata = corpus_inventory(root, args.query_count)
    for mode in args.modes.split(","):
        for repetition in range(args.repetitions):
            cache = work / "cache" / mode
            shutil.rmtree(cache, ignore_errors=True)
            session = open_workspace(args, root, mode, cache, rows, repetition, "cold_start")
            try:
                open_document(session,markers[queries[0]['at']],root,mode,repetition,rows)
                first = query(session, queries[0], markers, root, "first_query", mode, repetition, rows, False)
                first["cold_start_plus_query_ms"] = first["since_process_start_ms"]
                rows.append({**first, "phase": "cold_start_to_first_answer", "latency_ms": first["since_process_start_ms"],
                             "peak_tree_rss_kib": max([first["peak_tree_rss_kib"]] + [sample[1] for sample in session.samples])})
                opened={markers[queries[0]['at']]['file']}
                for spec in queries:
                    marker=markers[spec['at']]
                    if marker['file'] not in opened and len(opened)<args.open_documents:
                        open_document(session,marker,root,mode,repetition,rows);opened.add(marker['file'])
                if args.build_index:
                    response, measurement = session.call("workspace/index")
                    rows.append({"phase": "explicit_index_completion", "mode": mode, "repetition": repetition,
                                 **measurement, "response": response, "correct": None,
                                 'since_process_start_ms':(time.perf_counter()-session.started)*1000})
                for _ in range(args.warm_repetitions):
                    for spec in queries:
                        query(session, spec, markers, root, "warm_query", mode, repetition, rows, False)
                # Non-mutating disk benchmark: dirty overlay only; original repository remains unchanged.
                spec = queries[0]
                path = root / markers[spec["at"]]["file"]
                response, measurement = session.call("document/change", {"path": str(path), "text": path.read_text(errors="replace") + "\n// evaluation body-independent edit\n", "version": 2})
                row = query(session, spec, markers, root, "comment_edit", mode, repetition, rows, False)
                row["edit_to_answer_ms"] = measurement["latency_ms"] + row["latency_ms"]
                row["query_only_ms"] = row["latency_ms"]
                row["latency_ms"] = row["edit_to_answer_ms"]
                row["edit_response"] = response
                session.call("document/close", {"path": str(path)})
                rows.append({"phase": "final_stats", "mode": mode, "repetition": repetition,
                             "stats": session.stats(), "stderr": session.stderr[:], "correct": None})
            finally:
                session.close()
            restored = open_workspace(args, root, mode, cache, rows, repetition, "cache_restore_start")
            try:
                for spec in queries:
                    query(restored, spec, markers, root, "cache_restore_query", mode, repetition, rows, False)
            finally:
                restored.close()
    return metadata


def percentile(values, percent):
    if not values:
        return None
    ordered = sorted(values)
    # Nearest-rank estimator; small cold samples are labeled by n in reports.
    return ordered[max(0, math.ceil(percent * len(ordered)) - 1)]


def summarize(rows):
    groups = {}
    for row in rows:
        if "latency_ms" in row:
            groups.setdefault((row["mode"], row["phase"]), []).append(row)
    result = []
    for (mode, phase), group in sorted(groups.items()):
        measured = [row["latency_ms"] for row in group]
        verified = [row["latency_ms"] for row in group if row.get("correct") is True
                    and row.get('completeness')=='exact' and row.get('reason')!='stale_resolution_guard']
        checked = sum(row.get("correct") is not None for row in group)
        result.append({"mode": mode, "phase": phase, "n": len(group), "checked": checked,
                       "correct": sum(row.get("correct") is True for row in group),
                       "errors": sum(row.get("correct") is False for row in group),
                       "p50_ms_all_responses": percentile(measured, .50), "p95_ms_all_responses": percentile(measured, .95),
                       "p50_ms_exact_correct": percentile(verified, .50), "p95_ms_exact_correct": percentile(verified, .95),
                       "peak_tree_rss_kib": max(row["peak_tree_rss_kib"] for row in group),
                       "steady_tree_rss_kib": max(row["steady_tree_rss_kib"] for row in group),
                       "backend_peak_tree_rss_kib":max((row.get('backend_peak_tree_rss_kib',0) for row in group),default=0) or None,
                       "backend_steady_tree_rss_kib":max((row.get('backend_steady_tree_rss_kib',0) for row in group),default=0) or None,
                       "bridge_peak_rss_kib":max((row.get('bridge_peak_rss_kib',0) for row in group),default=0) or None,
                       "max_query_files_read":max((row.get('counter_delta',{}).get('filesRead',0) for row in group),default=0),
                       "max_query_bytes_read":max((row.get('counter_delta',{}).get('bytesRead',0) for row in group),default=0),
                       "max_query_files_parsed":max((row.get('counter_delta',{}).get('filesParsed',0) for row in group),default=0),
                       "compiler_launches_during_queries":sum(row.get('counter_delta',{}).get('compilerLaunches',0) for row in group),
                       "protocol_errors":sum('error' in row.get('response',{}) for row in group),
                       "unavailable_responses":sum(row.get('completeness')=='unavailable' for row in group),
                       "eligible_for_speedup": checked == len(group) and len(verified) == len(group)})
    return result


def compare(result, baseline, target):
    comparisons = []
    if baseline and result["workload"]["fingerprint"] != baseline.get("workload", {}).get("fingerprint"):
        raise ValueError("Baseline workload fingerprint differs; refusing an invalid speed comparison")
    for summary in result["summary"]:
        candidate = {"mode": summary["mode"], "phase": summary["phase"], "latency_ratio": None,
                     "memory_ratio": None, "target_ratio": target, "target_latency_ratio": result.get('configuration',{}).get('target_latency_ratio',2), "achieved": False,
                     "reason": "baseline_unavailable" if not baseline else "no_complete_matched_baseline"}
        if baseline and baseline.get("benchmark_kind") == "correctness-oracle":
            candidate["reason"] = "oracle_invalidation_policy_is_not_a_performance_baseline"
            comparisons.append(candidate)
            continue
        if baseline:
            matches = [item for item in baseline.get("summary", []) if item["phase"] == summary["phase"] and item.get("eligible_for_speedup")]
            if len(matches) == 1 and summary["eligible_for_speedup"]:
                other = matches[0]
                candidate_trace = sorted((row.get("query"), row.get("repetition")) for row in result.get("rows", []) if row.get("phase") == summary["phase"] and row.get("mode") == summary["mode"])
                baseline_trace = sorted((row.get("query"), row.get("repetition")) for row in baseline.get("rows", []) if row.get("phase") == other["phase"] and row.get("mode") == other["mode"])
                if candidate_trace == baseline_trace:
                    candidate["latency_ratio"] = other["p95_ms_exact_correct"] / max(.000001, summary["p95_ms_exact_correct"])
                    if result.get("memory_scope") == baseline.get("memory_scope"):
                        baseline_memory=other.get('backend_peak_tree_rss_kib') or other['peak_tree_rss_kib']
                        candidate_memory=summary.get('backend_peak_tree_rss_kib') or summary['peak_tree_rss_kib']
                        candidate["memory_ratio"] = baseline_memory / max(1, candidate_memory)
                        candidate['memory_scope_detail']='backend and observed descendants; artificial protocol bridge excluded when separately attributed'
                    candidate["achieved"] = candidate["memory_ratio"] is not None and candidate["latency_ratio"] >= candidate['target_latency_ratio'] and candidate["memory_ratio"] >= target
                    candidate["reason"] = "matched_complete_workload" if candidate["memory_ratio"] is not None else "different_memory_scopes"
                else:
                    candidate["reason"] = "different_query_trace"
        comparisons.append(candidate)
    return comparisons


def markdown(result):
    lines = ["# F# Navigator measured evaluation", "", f"Workload: `{result['workload']['kind']}`; fingerprint `{result['workload']['fingerprint']}`.",
             "", "A fast missing, approximate or unvalidated answer is not a successful query. Times below include every response; correctness is shown alongside them. No relative improvement is established without a complete matched baseline.", "",
             "| Mode | Phase | n | Correct / checked | p50 response ms | p95 response ms | Peak tree MiB |",
             "|---|---|---:|---:|---:|---:|---:|"]
    for row in result["summary"]:
        lines.append(f"| {row['mode']} | {row['phase']} | {row['n']} | {row['correct']} / {row['checked']} | {row['p50_ms_all_responses']:.3f} | {row['p95_ms_all_responses']:.3f} | {row['peak_tree_rss_kib']/1024:.2f} |")
    starts=[row for row in result['rows'] if row.get('phase')=='cold_start']
    if starts:
        lines += ['', 'Startup work counters (reported by the engine; project-file discovery is separate; snapshot validation may include assemblies and other artifacts):', '',
                  '| Mode | Run | Instrumented files read | Instrumented bytes read | Files parsed | Compiler launches | All zero |',
                  '|---|---:|---:|---:|---:|---:|---|']
        for row in starts:
            stats=row.get('response',{}).get('result',{})
            values=[stats.get(name,'unreported') for name in ('filesRead','bytesRead','filesParsed','compilerLaunches')]
            gates=row.get('startup_zero_source_work_gates',{})
            lines.append('| '+ ' | '.join(map(str,[row['mode'],row['repetition'],*values,bool(gates) and all(gates.values())]))+' |')
    lines += ['', '| Mode | Query phase | Maximum instrumented files read / query | Maximum instrumented bytes read / query | Maximum files parsed / query | Compiler launches |',
              '|---|---|---:|---:|---:|---:|']
    for row in result['summary']:
        if row['checked'] or 'query' in row['phase']:
            lines.append(f"| {row['mode']} | {row['phase']} | {row.get('max_query_files_read',0)} | {row.get('max_query_bytes_read',0)} | {row.get('max_query_files_parsed',0)} | {row.get('compiler_launches_during_queries',0)} |")
    opens=[row for row in result['rows'] if row.get('phase')=='document_open']
    if opens:
        lines += ['',f"Buffer-open gate: {sum(row.get('zero_ast_work') is True for row in opens)} / {len(opens)} document/open requests performed zero reported AST parses; {sum('error' in row.get('response',{}) for row in opens)} protocol errors."]
    lines += ["", "| Mode | Phase | p95 latency improvement | Peak memory improvement | Targets: memory / latency | Qualified |",
              "|---|---|---:|---:|---:|---|"]
    for row in result["comparisons"]:
        latency = f"{row['latency_ratio']:.2f}x" if row["latency_ratio"] is not None else "unavailable"
        memory = f"{row['memory_ratio']:.2f}x" if row["memory_ratio"] is not None else "unavailable"
        lines.append(f"| {row['mode']} | {row['phase']} | {latency} | {memory} | {row['target_ratio']:g}x / {row['target_latency_ratio']:g}x | {row['achieved']} ({row['reason']}) |")
    failures = {(row.get("mode"), row.get("query"), row.get("reason")) for row in result["rows"] if row.get("correct") is False}
    if failures:
        lines += ["", "## Failing cases", ""]
        lines += [f"- `{mode}` / `{query_name}`: {reason}" for mode, query_name, reason in sorted(failures)]
    lines += ["", "## Measurement limits", ""] + ["- " + note for note in result["measurement_notes"]]
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", required=True, help="Quoted command, e.g. 'target/release/fsharp-navigator daemon'")
    parser.add_argument("--engine-sha",help="Require an exact executable SHA-256 before taking the immutable run snapshot")
    parser.add_argument("--helper", help="Explicit .NET oracle command; never silently enabled")
    parser.add_argument("--evaluated-model", type=Path, help="Explicit previously evaluated model; preparation cost must be reported")
    parser.add_argument("--library-catalog", type=Path, help="Explicit pre-extracted binary signature/documentation catalog")
    parser.add_argument("--builtin-environment",type=Path)
    parser.add_argument("--decompiler")
    parser.add_argument("--semantic-snapshot", type=Path, help="Explicit opt-in full semantic occurrence snapshot; bootstrap cost must be reported")
    parser.add_argument("--setup-report", type=Path, help="Measured preparation report produced by prepare.py")
    parser.add_argument("--modes", default="indexed", help="Comma-separated lexical,semantic,indexed")
    parser.add_argument("--repetitions", type=int, default=3)
    parser.add_argument("--warm-repetitions", type=int, default=3)
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--sample-interval", type=float, default=.01)
    parser.add_argument("--memory-limit-mib", type=int, default=4096, help="Abort the measured process tree above this sampled RSS;0 disables")
    parser.add_argument("--memory-scope", choices=["language-service", "whole-ide"], default="language-service")
    parser.add_argument("--benchmark-kind", choices=["candidate", "ionide", "correctness-oracle"], default="candidate")
    parser.add_argument("--corpus", help="Pinned Git corpus root; results unvalidated until a semantic baseline is provided")
    parser.add_argument("--query-count", type=int, default=30)
    parser.add_argument("--open-documents", type=int, default=12, help="Open up to N distinct ordinary buffers; require no eager AST work")
    parser.add_argument("--build-index", action="store_true")
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--target-ratio", type=float, default=100, help="Memory reduction target; latency has a separate target")
    parser.add_argument("--target-latency-ratio", type=float, default=2, help="Provisional latency improvement target; not a measured achievement")
    parser.add_argument("--output", type=Path, default=ROOT / "evaluation" / "results" / "latest.json")
    parser.add_argument("--work", type=Path)
    parser.add_argument("--fail-on-error", action="store_true", help="Exit 1 if any checked answer fails")
    parser.add_argument("--only", help="Comma-separated fixture query names for a focused reproduction")
    parser.add_argument("--manifest",type=Path,default=ROOT/'fixtures'/'expectations.json')
    parser.add_argument("--fixture-source",type=Path,default=ROOT/"fixtures"/"sources")
    parser.add_argument("--edit-reset",choices=["change","close"],default="change")
    parser.add_argument("--first-query",help="Selected fixture query to measure before any other navigation request")
    parser.add_argument("--correctness-only", action="store_true", help="Skip warm/edit/restart phases; useful for expensive explicit oracle checks")
    parser.add_argument("--edits-only", action="store_true", help="Run first query and selected dirty-edit checks without the clean/warm/restart matrix")
    parser.add_argument("--edit-names", help="Comma-separated dirty-edit names; other edits are skipped")
    parser.add_argument("--include-declaration", action="store_true", help="Use declaration-inclusive reference sets on both engines; needed for FSAC's LSP reference behavior")
    parser.add_argument("--reference-normalization",choices=['strict','vscode'],default='strict',help='Pinned VS Code UI removes identical reference locations; raw duplicates remain recorded')
    args = parser.parse_args()
    if args.correctness_only and args.edits_only:
        parser.error('--correctness-only and --edits-only are mutually exclusive')
    if (ROOT/'evaluation'/'.runtime'/'pause-measurements').exists():
        parser.error('Measurement coordinator paused new runs; remove evaluation/.runtime/pause-measurements after review completes')
    if min(args.repetitions, args.warm_repetitions, args.query_count) < 1:
        parser.error("Repetitions and query count must be positive")
    temporary_root=ROOT/'evaluation'/'.runtime'/'runs'
    temporary_root.mkdir(parents=True,exist_ok=True)
    work = args.work or Path(tempfile.mkdtemp(prefix="fsharp-navigator-eval-",dir=temporary_root))
    work = work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    free_before=shutil.disk_usage(work)
    provenance = snapshot_engine(args, work)
    rows, started = [], time.perf_counter()
    metadata = run_corpus(args, work, rows) if args.corpus else run_fixtures(args, work, rows)
    benchmark_kind = "correctness-oracle" if "oracle_adapter.py" in args.engine else args.benchmark_kind
    result = {"schemaVersion": 1, "engine_command": shlex.split(args.engine), "helper_command": args.helper,
              "benchmark_kind": benchmark_kind,
              "provenance": {**provenance, "source_sha256_after":source_sha(),
                             "prepared_input_sha256_after":{name:file_sha(path) for name,path in [('model',args.evaluated_model),('catalog',args.library_catalog),('builtin_environment',args.builtin_environment),('semantic_snapshot',args.semantic_snapshot)] if path},
                             "source_changed_during_run":provenance['source_sha256_before']!=source_sha()},
              "oracle_invalidation": "before every query; unsuitable for warmed compiler comparisons" if benchmark_kind == "correctness-oracle" else None,
              "prepared_inputs": {"model": str(args.evaluated_model) if args.evaluated_model else None,
                                  "catalog": str(args.library_catalog) if args.library_catalog else None,
                                  "builtin_environment": str(args.builtin_environment) if args.builtin_environment else None,
                                  "semantic_snapshot": str(args.semantic_snapshot) if args.semantic_snapshot else None,
                                  "setup": json.loads(args.setup_report.read_text()) if args.setup_report else None},
              "platform": sys.platform, "memory_scope": args.memory_scope, "workload": metadata, "elapsed_seconds": time.perf_counter() - started,
              "work_storage":{"path":str(work),"device":os.stat(work).st_dev,"total_bytes":free_before.total,
                              "free_bytes_before":free_before.free,"free_bytes_after":shutil.disk_usage(work).free},
              "machine": {"kernel": os.uname().release, "architecture": os.uname().machine,
                          "logical_cpus": os.cpu_count(), "load_average_after_run": os.getloadavg(),
                          "mem_total_kib": next((int(line.split()[1]) for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith('MemTotal:')), None)},
              "configuration": {"repetitions": args.repetitions, "warm_repetitions": args.warm_repetitions,
                                "sample_interval_seconds": args.sample_interval, "target_ratio": args.target_ratio,
                                "target_latency_ratio": args.target_latency_ratio,
                                "open_documents":args.open_documents,
                                "manifest":str(args.manifest), "only":args.only, "first_query":args.first_query,
                                "correctness_only":args.correctness_only, "edits_only":args.edits_only,
                                "edit_names":args.edit_names,
                                "edit_reset":args.edit_reset,
                                "include_declaration":args.include_declaration,
                                "reference_normalization":args.reference_normalization,
                                "memory_limit_mib": args.memory_limit_mib},
              "rows": rows, "summary": summarize(rows), "measurement_notes": [
                  "Process-cold, cache-empty runs do not flush the operating-system page cache; these are not disk-cold measurements.",
                  "Latency uses a monotonic clock. Cold startup includes process spawn and workspace/open; cold_start_plus_query_ms is also stored per first query.",
                  "Peak RSS sums the daemon and every observed descendant from Linux /proc PPid scanning. Shared pages are counted in each process; sampling can miss short peaks/short-lived children. Empty task/children files are not used.",
                  "The Python measurement controller is outside the language-service process tree. Whole VS Code/Rider application footprints are not measured.",
                  "Process RSS excludes filesystem/page-cache storage. In particular, a tmpfs-backed index consumes system memory outside the measured process RSS; this is not a whole-system memory measurement.",
                  "steady_tree_rss_kib means immediate post-response RSS, not a forced-GC or long-idle measurement.",
                  "p95 uses nearest-rank estimation; inspect n before interpreting small cold-start samples.",
                  "This is a shared development machine without CPU isolation. Concurrent build/test work can influence latency; comparative claims need controlled repeated runs.",
                  "Restore/build/project-evaluation setup is not fabricated. Explicit helper use is measured inside its subprocess tree; no Ionide/Rider denominator is assumed.",
                  "Raw responses, validation failures and work counters are retained. Corpus queries without a semantic oracle never qualify as performance wins."]}
    result['measurement_notes'].append('filesRead/bytesRead include source reads and, when an imported semantic snapshot is used, assembly/project/artifact fingerprint validation. They are not universally F#-source-only counters; metadata import may have additional separately measured IO.')
    baseline = json.loads(args.baseline.read_text()) if args.baseline else None
    result["comparisons"] = compare(result, baseline, args.target_ratio)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    args.output.with_suffix(".md").write_text(markdown(result))
    print(json.dumps({"result": str(args.output.resolve()), "checked": sum(x.get("correct") is not None for x in rows),
                      "failed": sum(x.get("correct") is False for x in rows), "elapsed_seconds": result["elapsed_seconds"]}))
    if args.work is None:
        shutil.rmtree(work)
    if args.fail_on_error and any(row.get("correct") is False for row in rows):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
