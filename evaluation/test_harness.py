"""Guard the evaluator against false correctness/speedup claims."""
import tempfile
import json
import subprocess
import sys
import unittest
from pathlib import Path

import run


class EvaluationContractTests(unittest.TestCase):
    def test_real_child_grandchild_rss_and_pid_identity(self):
        grandchild="import sys; payload=bytearray(24*1024*1024); print('ready',flush=True); sys.stdin.read()"
        child=f"import subprocess,sys,json; payload=bytearray(16*1024*1024); p=subprocess.Popen([sys.executable,'-c',{grandchild!r}],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True); p.stdout.readline(); print(json.dumps([p.pid]),flush=True); sys.stdin.read(); p.communicate()"
        bridge=f"import subprocess,sys,json; p=subprocess.Popen([sys.executable,'-c',{child!r}],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True); descendants=json.loads(p.stdout.readline()); print(json.dumps([p.pid]+descendants),flush=True); sys.stdin.read(); p.communicate()"
        process=subprocess.Popen([sys.executable,'-c',bridge],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
        try:
            descendants=json.loads(process.stdout.readline())
            identity=run.process_identity(process.pid)[1]
            total,pids,residents,parents,identities=run.process_tree_details(process.pid,identity)
            self.assertEqual(set(pids),{process.pid,*descendants})
            self.assertGreater(total,48*1024)
            self.assertGreater(run.subtree_rss(descendants[0],residents,parents,identities,identities[descendants[0]]),40*1024)
            self.assertEqual(run.process_tree_details(process.pid,identity+1)[0],0)
            self.assertEqual(run.subtree_rss(descendants[0],residents,parents,identities,identities[descendants[0]]+1),0)
        finally:
            process.communicate(timeout=5)
    def test_observed_reparented_worker_stays_owned(self):
        with tempfile.TemporaryDirectory() as directory:
            flag=Path(directory)/'stop'
            grandchild=f"import pathlib,time; payload=bytearray(4*1024*1024); print('ready',flush=True)\nwhile not pathlib.Path({str(flag)!r}).exists(): time.sleep(.01)"
            child=f"import subprocess,sys,json; p=subprocess.Popen([sys.executable,'-c',{grandchild!r}],stdout=subprocess.PIPE,text=True); p.stdout.readline(); print(json.dumps([p.pid]),flush=True); sys.stdin.readline()"
            bridge=f"import subprocess,sys,json; p=subprocess.Popen([sys.executable,'-c',{child!r}],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True); descendants=json.loads(p.stdout.readline()); print(json.dumps([p.pid]+descendants),flush=True); sys.stdin.readline(); p.stdin.close(); p.wait(); print('orphaned',flush=True); sys.stdin.read()"
            process=subprocess.Popen([sys.executable,'-c',bridge],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
            try:
                child_pid,worker_pid=json.loads(process.stdout.readline())
                identity=run.process_identity(process.pid)[1];owned={process.pid:identity}
                self.assertIn(worker_pid,run.process_tree_details(process.pid,identity,owned)[1]);worker_identity=owned[worker_pid]
                process.stdin.write('orphan\n');process.stdin.flush();self.assertEqual(process.stdout.readline().strip(),'orphaned')
                parent,current_identity=run.process_identity(worker_pid);self.assertNotEqual(parent,child_pid);self.assertEqual(current_identity,worker_identity)
                self.assertIn(worker_pid,run.process_tree_details(process.pid,identity,owned)[1]);owned[worker_pid]=worker_identity+1
                self.assertNotIn(worker_pid,run.process_tree_details(process.pid,identity,owned)[1])
                self.assertNotIn(worker_pid,owned)
            finally:
                flag.write_text('stop');process.communicate(timeout=5)

    def test_fixture_materialization_preserves_crlf_and_utf16(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);source=root/'source';source.mkdir()
            (source/'A.fs').write_bytes('module A\r\nlet text = "😀"; (*@target*)value\r\n'.encode())
            marks=run.materialize(root/'work',source)
            self.assertEqual((root/'work/A.fs').read_bytes(),'module A\r\nlet text = "😀"; value\r\n'.encode())
            self.assertEqual(marks['target']['line'],1);self.assertEqual(marks['target']['character'],17)

    def test_markers_remove_comments_and_preserve_utf16(self):
        source, markers = run.strip_markers('let s = "😀"; (*@target*)value\n', "A.fs")
        self.assertEqual(source, 'let s = "😀"; value\n')
        self.assertEqual(markers["target"]["character"], 14)

    def test_approximate_correct_looking_answer_is_failure(self):
        spec = {"method": "definition", "expected": ["def"]}
        marks = {"def": {"file": "A.fs", "line": 0, "character": 4, "endCharacter": 5}}
        reply = {"result": {"completeness": "approximate", "locations": []}}
        self.assertFalse(run.assess(spec, reply, marks, Path("/tmp"))["correct"])

    def test_extra_or_missing_references_fail(self):
        marks = {"def": {"file": "A.fs", "line": 0, "character": 4, "endCharacter": 5}}
        spec = {"method": "references", "expected": ["def"]}
        location = {"uri": "file:///tmp/A.fs", "range": {"start": {"line": 0, "character": 4}, "end": {"line": 0, "character": 5}}}
        self.assertTrue(run.assess(spec, {"result": {"completeness": "exact", "locations": [location]}}, marks, Path("/tmp"))["correct"])
        self.assertFalse(run.assess(spec, {"result": {"completeness": "exact", "locations": []}}, marks, Path("/tmp"))["correct"])
        self.assertFalse(run.assess(spec, {"result": {"completeness": "exact", "locations": [location, location]}}, marks, Path("/tmp"))["correct"])
        spec['referenceNormalization']='vscode'
        normalized=run.assess(spec,{"result":{"completeness":"exact","locations":[location,location]}},marks,Path('/tmp'))
        self.assertTrue(normalized['correct'])
        self.assertEqual(normalized['duplicate_location_count'],1)
        distinct={'uri':'file:///tmp/A.fs','range':{'start':{'line':0,'character':6},'end':{'line':0,'character':7}}}
        self.assertFalse(run.assess(spec,{'result':{'completeness':'exact','locations':[location,distinct]}},marks,Path('/tmp'))['correct'])

    def test_generic_declaration_is_not_contextual_hover_equivalence(self):
        spec={'method':'hover','expectHover':True,'expectGenericArguments':{"'T":'int',"'U":'string'}}
        result={'completeness':'exact','hover':{'markdown':"val map: ('T -> 'U) -> 'T list -> 'U list\n\nExamples mention int and string"}}
        self.assertFalse(run.assess(spec,{'result':result},{},Path('/tmp'))['correct'])
        result['hover']['markdown'] += "\n\n**Generic Parameters**\n* `'T` is `int`\n* `'U` is `string`"
        self.assertTrue(run.assess(spec,{'result':result},{},Path('/tmp'))['correct'])
        result['hover']['markdown']=result['hover']['markdown'].replace("`'T` is `int`","`'T` is `bool`")
        self.assertFalse(run.assess(spec,{'result':result},{},Path('/tmp'))['correct'])
        spec={'method':'hover','expectHover':True,'expectPropertyResult':{'name':'Item1','type':'Person'}}
        result['hover']['markdown']="member Item1 : 'T1\n\nPerson might appear in documentation"
        self.assertFalse(run.assess(spec,{'result':result},{},Path('/tmp'))['correct'])
        result['hover']['markdown']='property Item1: Example.Person'
        self.assertTrue(run.assess(spec,{'result':result},{},Path('/tmp'))['correct'])

    def test_unvalidated_or_failing_answers_cannot_win(self):
        rows = [{"mode": "indexed", "phase": "warm", "latency_ms": .01, "peak_tree_rss_kib": 10,
                 "steady_tree_rss_kib": 10, "correct": None}]
        summary = run.summarize(rows)
        self.assertFalse(summary[0]["eligible_for_speedup"])
        result = {"workload": {"fingerprint": "a"}, "summary": summary}
        self.assertIsNone(run.compare(result, None, 100)[0]["latency_ratio"])
        with self.assertRaises(ValueError):
            run.compare(result, {"workload": {"fingerprint": "different"}}, 100)

    def test_expected_unavailable_freshness_guard_is_not_a_speed_win(self):
        rows=[{'mode':'indexed','phase':'rename_guard','latency_ms':.01,'peak_tree_rss_kib':10,
               'steady_tree_rss_kib':10,'correct':True,'completeness':'unavailable','reason':'stale_resolution_guard'}]
        summary=run.summarize(rows)[0]
        self.assertEqual(summary['correct'],1)
        self.assertFalse(summary['eligible_for_speedup'])
        self.assertIsNone(summary['p50_ms_exact_correct'])

    def test_all_fixture_markers_materialize(self):
        with tempfile.TemporaryDirectory() as directory:
            markers = run.materialize(Path(directory))
            self.assertGreater(len(markers), 30)
            for source in Path(directory).rglob("*.fs"):
                self.assertNotIn("(*@", source.read_text())

    def test_qualified_range_alternatives_preserve_exact_occurrence_count(self):
        marks={'member':{'file':'A.fs','line':0,'character':7,'endCharacter':11},
               'receiver':{'file':'A.fs','line':0,'character':0,'endCharacter':6}}
        spec={'method':'references','expected':[{'at':'member','qualifiedFrom':['receiver']}]}
        full={'uri':'file:///tmp/A.fs','range':{'start':{'line':0,'character':0},'end':{'line':0,'character':11}}}
        token={'uri':'file:///tmp/A.fs','range':{'start':{'line':0,'character':7},'end':{'line':0,'character':11}}}
        for span in (full,token):
            self.assertTrue(run.assess(spec,{'result':{'completeness':'exact','locations':[span]}},marks,Path('/tmp'))['correct'])
        self.assertFalse(run.assess(spec,{'result':{'completeness':'exact','locations':[full,token]}},marks,Path('/tmp'))['correct'])

    def test_cross_project_rename_recalculates_markers_without_writing_disk(self):
        manifest=json.loads((run.ROOT/'fixtures'/'expectations.common-fields.json').read_text())
        edit=next(item for item in manifest['edits'] if item['name']=='cross_project_field_rename')
        with tempfile.TemporaryDirectory() as directory:
            workspace=Path(directory)
            markers=run.materialize(workspace)
            before=(workspace/'Library/RecordFlows.fs').read_text()
            changes,updated=run.prepare_edit(edit,markers,workspace)
            self.assertEqual(len(changes),2)
            self.assertEqual(updated['customer_name_def']['token'],'Label')
            self.assertEqual(updated['supplier_name_def']['token'],'Name')
            self.assertEqual(updated['customer_home_def']['character'],markers['customer_home_def']['character']+1)
            self.assertEqual((workspace/'Library/RecordFlows.fs').read_text(),before)

    def test_edit_reference_expectations_move_only_selected_occurrence(self):
        manifest=json.loads((run.ROOT/'fixtures'/'expectations.common-fields.json').read_text())
        edit=next(item for item in manifest['edits'] if item['name']=='receiver_alias_edit')
        checks=run.edit_checks(edit,manifest['queries'])
        def names(spec):return {item if isinstance(item,str) else item['at'] for item in spec['expected']}
        self.assertNotIn('customer_r23',names(checks[1]))
        self.assertIn('customer_r23',names(checks[2]))
        original=next(q for q in manifest['queries'] if q['name']=='customer_all_references')
        self.assertIn('customer_r23',names(original))


if __name__ == "__main__":
    unittest.main()
