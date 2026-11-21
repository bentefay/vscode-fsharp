#!/usr/bin/env python3
"""Package local prototype source plus completed evaluation evidence."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import subprocess
import zipfile

root=Path(__file__).resolve().parents[1]
arguments=argparse.ArgumentParser(description=__doc__)
arguments.add_argument('--output',type=Path,help='Explicit checkpoint ZIP path; default uses the package version.')
options=arguments.parse_args()
excluded={'.git','.tools','.cache','node_modules','target','.devenv','.direnv','__pycache__','.runtime','.fsharp-navigator','.vscode-test','obj'}
source_roots={'cli','crates','docs','evaluation','extension','fixtures','nix','scripts','semantic','vendor'}
root_files={'.envrc','.gitignore','.npmrc','.vscodeignore','AGENTS.md','Cargo.toml','Cargo.lock','LICENSE','README.md','THIRD_PARTY_NOTICES.md','devenv.nix','devenv.yaml','devenv.lock','global.json','package.json','package-lock.json','rust-toolchain.toml'}
try:
    paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=root,stderr=subprocess.DEVNULL).decode().split('\0')
    files={root/p for p in paths if p}
except subprocess.CalledProcessError:
    files=set()
    for directory, children, names in os.walk(root):
        relative=Path(directory).relative_to(root)
        children[:]=[name for name in children if name not in excluded and name not in {'artifacts','test-results'} and (name!='bin' or relative==Path('cli')) and not (relative==Path('extension') and name=='dist')]
        files.update(Path(directory)/name for name in names)
# Git ignores the expanded generated parsers, but a source checkpoint must keep
# the exact native build inputs as well as their compressed transport form.
parser_manifest=json.loads((root/'scripts/generated-parser-inputs/manifest.json').read_text())
for pin in parser_manifest['parsers']:
    generated=root/pin['source']
    if generated.is_symlink() or not generated.is_file():
        raise RuntimeError('Run node scripts/restore-parser-sources.mjs before packaging source.')
    with generated.open('rb') as stream:
        actual_hash=hashlib.file_digest(stream,'sha256').hexdigest()
    if generated.stat().st_size!=pin['sourceBytes'] or actual_hash!=pin['sourceSha256']:
        raise RuntimeError(f'Generated parser does not match pinned source: {pin["source"]}')
    files.add(generated)
results=root/'evaluation'/'results'
if results.is_dir():
    files.update(p for p in results.rglob('*') if p.is_file())
evidence_categories={'host-environment','verification','cancellation','index-storage','recovery', 'inventory-memory','resolution-states','prepared-indexes','shared-import'}
for category in evidence_categories:
    evidence=root/'test-results'/category
    if evidence.is_dir():
        files.update(p for p in evidence.rglob('*') if p.is_file())
def compiled_binary(path):
    if path.is_symlink() or not path.is_file():return False
    with path.open('rb') as stream:magic=stream.read(4)
    # Independent probes retain source, commands and hashes; their executable
    # build products do not belong in a source/evidence checkpoint.
    return magic in {b'\x7fELF',b'\x00asm',b'\xfe\xed\xfa\xce',b'\xce\xfa\xed\xfe',b'\xfe\xed\xfa\xcf',b'\xcf\xfa\xed\xfe'}
for p in list(files):
    relative=p.relative_to(root)
    if len(relative.parts)==1 and relative.name not in root_files:files.discard(p)
    elif len(relative.parts)>1 and relative.parts[0] not in source_roots|{'test-results'}:files.discard(p)
    elif relative.parts[0]=='vendor' and relative.parts[1]!='tree-sitter-fsharp':files.discard(p)
    elif any(part in excluded for part in relative.parts):files.discard(p)
    elif 'bin' in relative.parts and relative.parts[:2]!=('cli','bin'):files.discard(p)
    elif relative.parts[0] in {'artifacts','bin'}:files.discard(p)
    elif relative.parts[:2]==('extension','dist'):files.discard(p)
    elif relative.parts[0]=='test-results' and relative.parts[1] not in evidence_categories:files.discard(p)
    # Shared-import reports retain complete canonical/raw digests and producer
    # certificates. The four arms' duplicate generated fact payloads are
    # disposable reproduction inputs, not additional source/evidence reports.
    elif relative.parts[:2]==('test-results','shared-import') and (relative.name in {'environment.json','builtins.json','libraries.json'} or relative.name.startswith('catalog-') or 'catalogs' in relative.parts):files.discard(p)
    elif relative.suffix=='.vsix':files.discard(p)
    elif relative.suffix in {'.sqlite','.sqlite3','.db','.dll','.exe','.pdb'} or relative.name.endswith(('-wal','-shm')):files.discard(p)
    elif compiled_binary(p):files.discard(p)
version=json.loads((root/'package.json').read_text())['version']
artifact=options.output or root/'artifacts'/f'fsharp-navigator-{version}-source.zip'
if not artifact.is_absolute():artifact=root/artifact
artifact.parent.mkdir(parents=True,exist_ok=True)
for required in ['cli/bin/run','cli/bin/run.mjs','Cargo.lock','package-lock.json','devenv.lock','extension/src/extension.ts','crates/navigator-daemon/src/main.rs']:
    if root/required not in files:raise RuntimeError(f'Source package is missing required file {required}')
large_generated={
    'vendor/tree-sitter-fsharp/fsharp/src/parser.c': (59270973,'00943a441bd59446ed9d321f3e0e09402ec3069c623890fc0b1893717d001302'),
}
oversized=[]
for p in files:
    if not p.is_file() or p.stat().st_size<=20*1024*1024:continue
    relative=str(p.relative_to(root));expected=large_generated.get(relative)
    if expected is None or p.stat().st_size!=expected[0] or hashlib.sha256(p.read_bytes()).hexdigest()!=expected[1]:oversized.append(relative)
if oversized:raise RuntimeError(f'Unexpected oversized source package files: {oversized}')
with zipfile.ZipFile(artifact,'w',zipfile.ZIP_DEFLATED) as z:
    for p in sorted(files):
        if p.is_file() and not p.is_symlink():z.write(p,Path('fsharp-navigator')/p.relative_to(root))
digest=hashlib.sha256(artifact.read_bytes()).hexdigest()
artifact.with_suffix('.zip.sha256').write_text(f'{digest}  {artifact.name}\n')
print(f'{artifact}\nSHA-256 {digest}\nFiles {len(files)}')
