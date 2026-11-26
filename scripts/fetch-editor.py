#!/usr/bin/env python3
"""Provision a fresh disposable copy of the pinned official Linux editor."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import tarfile
import tempfile
import urllib.request

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--version', default='1.145.3')
args = parser.parse_args()
if platform.system() != 'Linux' or platform.machine() != 'x86_64':
    raise SystemExit('This provisioner is currently Linux x64-only.')
pin = json.loads((root/'extension/test/runtime-lock.json').read_text())['vscode'][args.version]['linux-x64']
cache = root/'.cache'; cache.mkdir(exist_ok=True)
archive = cache / f'code-{args.version}-{pin["sha256"]}.tar.gz'
if not archive.exists():
    with tempfile.TemporaryDirectory(dir=cache) as temp:
        part = Path(temp)/'download'
        with urllib.request.urlopen(pin['url'], timeout=60) as response, part.open('wb') as out:
            total = 0
            while chunk := response.read(1024*1024):
                total += len(chunk)
                if total > 512*1024*1024:
                    raise ValueError('Editor archive exceeds size limit')
                out.write(chunk)
        if hashlib.file_digest(part.open('rb'), 'sha256').hexdigest() != pin['sha256']:
            raise ValueError('Editor archive checksum mismatch')
        part.replace(archive)
with archive.open('rb') as stream:
    if hashlib.file_digest(stream, 'sha256').hexdigest() != pin['sha256']:
        raise ValueError('Cached editor archive checksum mismatch')
destination = Path(tempfile.mkdtemp(prefix='editor-runtime-', dir=cache))
with tarfile.open(archive) as tar:
    tar.extractall(destination, filter='data')
binary = destination/'VSCode-linux-x64/code'
if not binary.is_file():
    raise ValueError('Official editor executable missing')
evidence = {'version': args.version, 'archiveSha256': pin['sha256'], 'executable': str(binary),
            'executableSha256': hashlib.file_digest(binary.open('rb'), 'sha256').hexdigest()}
(cache/'editor-runtime.json').write_text(json.dumps(evidence, indent=2)+'\n')
print(json.dumps(evidence, indent=2))
