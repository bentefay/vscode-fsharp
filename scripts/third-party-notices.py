#!/usr/bin/env python3
"""Collect locked Rust dependency notices for the distributed native binary."""
import json
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[1]
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1'], cwd=root))
lines = ['# Third-party notices', '', 'The native executable uses the following locked Rust packages. This list includes build and platform dependencies for transparency. The VSIX does not bundle the .NET helper, SDK, editor runtime or Node development dependencies.', '']
for package in sorted(metadata['packages'], key=lambda item: (item['name'], item['version'])):
    if package['id'] in metadata['workspace_members']:
        continue
    directory = Path(package['manifest_path']).parent
    licenses = sorted(file for file in directory.iterdir() if file.is_file() and (file.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE'))))
    if package.get('license_file'):
        specified = directory / package['license_file']
        if specified.is_file() and specified not in licenses:
            licenses.append(specified)
    lines += [f"## {package['name']} {package['version']}", '', f"License: {package.get('license') or 'see bundled notice'}. Source: {package.get('repository') or package.get('source') or 'Cargo registry'}", '']
    if not licenses:
        lines += ['The package declares the license above but contains no top-level license text. Its Cargo source archive remains available from the pinned registry dependency.', '']
    for file in licenses:
        lines += [f'### {file.name}', '', '```text', file.read_text(errors='replace').rstrip(), '```', '']
(root / 'THIRD_PARTY_NOTICES.md').write_text('\n'.join(lines))
print(f'Wrote notices for {len(metadata["packages"]) - len(metadata["workspace_members"])} packages')
