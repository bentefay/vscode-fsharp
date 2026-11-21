#!/usr/bin/env python3
"""Real compiler checks for conditional projection rules, independent of Rust."""
from pathlib import Path
import hashlib,json,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[2]
DOTNET=ROOT/'.tools/dotnet/dotnet'
cases=[
 ('inactive_unclosed_string',[], '#if MISSING\nlet broken = "unterminated\n#else\nlet value = 2\n#endif\nprintfn "%d" value\n',0,'2'),
 ('inactive_unclosed_comment',[], '#if MISSING\n(* unterminated\n#else\nlet value = 2\n#endif\nprintfn "%d" value\n',0,'2'),
 ('true_is_undefined_identifier',[], '#if true\nlet value = 1\n#else\nlet value = 2\n#endif\nprintfn "%d" value\n',0,'2'),
 ('true_can_be_defined_identifier',['--define:true'], '#if true\nlet value = 1\n#else\nlet value = 2\n#endif\nprintfn "%d" value\n',0,'1'),
 ('elif_preview',['--langversion:preview','--define:SECOND'], '#if FIRST\nlet value = 1\n#elif SECOND\nlet value = 2\n#else\nlet value = 3\n#endif\nprintfn "%d" value\n',0,'2'),
 ('elif_v10_rejected',['--langversion:10.0','--define:SECOND'], '#if FIRST\nlet value = 1\n#elif SECOND\nlet value = 2\n#else\nlet value = 3\n#endif\nprintfn "%d" value\n',1,''),
 ('active_multiline_string',[], 'let text = """\n#if MISSING\n#else\n#endif\n"""\nlet value = 2\nprintfn "%d" value\n',0,'2'),
 ('active_nested_comment',[], '(* outer\n (* inner *)\n#if MISSING\n#else\n#endif\n*)\nlet value = 2\nprintfn "%d" value\n',0,'2'),
]
base=Path(tempfile.mkdtemp(prefix='conditional-oracles-',dir=ROOT/'.tmp'));rows=[]
for name,args,source,expected_exit,expected_output in cases:
 p=base/(name+'.fsx');p.write_text(source)
 r=subprocess.run([str(DOTNET),'fsi',*args,'--exec',str(p)],text=True,capture_output=True,timeout=30)
 row={'id':name,'args':args,'source':source,'exitCode':r.returncode,'stdout':r.stdout,'stderr':r.stderr,'expectedExit':expected_exit,'expectedOutput':expected_output,'matched':r.returncode==expected_exit and r.stdout.strip()==expected_output};rows.append(row)
 print(name,row['matched'],r.returncode,r.stdout.strip(),flush=True)
report={'provenance':'Fresh real FSI after recovery. This is a lexer/language-version oracle, not project defines certification or a native correctness/performance result.','dotnetSha256':hashlib.sha256(DOTNET.read_bytes()).hexdigest(),'fixtureRoot':str(base),'cases':rows}
(ROOT/'docs/review/conditional-oracles.json').write_text(json.dumps(report,indent=2)+'\n')
