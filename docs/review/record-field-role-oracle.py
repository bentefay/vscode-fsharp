#!/usr/bin/env python3
"""FCS occurrence-role controls, including semantic-error recovery boundaries."""
import hashlib, json, os, re, subprocess, tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
DOTNET=ROOT/'.tools/dotnet/dotnet'
ENV=dict(os.environ,DOTNET_ROOT=str(DOTNET.parent),TMPDIR=str(ROOT/'.tmp'),DOTNET_NOLOGO='1')
HEAD='type R = { Artifact:int }\n'
CASES={
 'ordinary_open':HEAD+'module Artifact =\n    type Item = Item of int\nopen Artifact\nlet read (r:R) = r.Artifact\n',
 'unresolved_open':HEAD+'open Missing.Artifact\nlet read (r:R) = r.Artifact\n',
 'open_type':HEAD+'module Types =\n    type Artifact() = class end\nopen type Types.Artifact\nlet read (r:R) = r.Artifact\n',
 'unresolved_open_type':HEAD+'open type R.Artifact\nlet read (r:R) = r.Artifact\n',
 'qualified_type_head':HEAD+'module Artifact =\n    type Item = Item of int\nlet read (x:Artifact.Item) = x\n',
 'unresolved_qualified_type_head':HEAD+'let read (x:Missing.Artifact) = x\n',
 'unresolved_record_type_path':HEAD+'let read (x:R.Artifact) = x\n',
 'generic_type_argument':HEAD+'type Artifact = string\nlet read (x:list<Artifact>) = x\n',
 'unresolved_generic_argument':HEAD+'let read (x:list<Artifact>) = x\n',
 'alias_rhs':HEAD+'module Types =\n    type Artifact = string\ntype Alias = Types.Artifact\nlet read (r:R) = r.Artifact\n',
 'field_expression_and_nameof':HEAD+'let read (r:R) = r.Artifact\nlet label (r:R) = nameof r.Artifact\nlet parenthesized (r:R) = nameof (r.Artifact)\n',
 'static_nameof_field':HEAD+'let label = nameof R.Artifact\n',
 'unresolved_type_inside_nameof':HEAD+'let label = nameof (typeof<R.Artifact>)\n',
 'unresolved_explicit_constructor':HEAD+'let value = new R.Artifact()\n',
 'record_pattern_and_literal':HEAD+'let value = {Artifact=1}\nlet read {Artifact=value} = value\n',
}

def main():
 base=Path(tempfile.mkdtemp(prefix='record-field-role-',dir=ROOT/'.tmp'));rows=[]
 for name,source in CASES.items():
  path=base/(name+'.fsx');path.write_text(source);queries=[]
  for line,text in enumerate(source.splitlines()):
   for n,match in enumerate(re.finditer(r'\bArtifact\b',text)):
    queries.append(dict(label=f'Artifact_{line}_{n}',token='Artifact',line=line,character=match.start()))
  rows.append(dict(case=name,path=str(path),source=source,queries=queries))
 manifest=base/'cases.json';manifest.write_text(json.dumps(rows));p=subprocess.run([str(DOTNET),'fsi','--exec',str(ROOT/'docs/review/alias-expression-oracle.fsx'),str(manifest)],env=ENV,text=True,capture_output=True,timeout=120)
 answers=[json.loads(line) for line in p.stdout.splitlines() if line.startswith('{')]
 assert p.returncode==0 and len(answers)==len(rows),p.stderr
 report=dict(provenance='Actual pinned FCS occurrence identity and reference groups. Semantic-invalid cases retained specifically to check role disjointness under error recovery, not counted as valid-code coverage.',fixtureRoot=str(base),dotnetSha256=hashlib.sha256(DOTNET.read_bytes()).hexdigest(),cases=rows,fcs=answers,fcsProcess=dict(exitCode=p.returncode,stderr=p.stderr))
 (ROOT/'docs/review/record-field-role-oracle.json').write_text(json.dumps(report,indent=2)+'\n')
 for row in answers:
  print(row['case'],'valid='+str(row['valid']),[(r['line'],(r['symbol'] or {}).get('kind'),(r['symbol'] or {}).get('fullName')) for r in row['results']])
if __name__=='__main__':main()
