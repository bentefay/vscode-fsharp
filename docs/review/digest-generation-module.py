#!/usr/bin/env python3
"""Run independent integration tests against an unchanged production module copy."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--source', type=Path, default=ROOT / 'crates/navigator-core/src/validation.rs')
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    base = Path(tempfile.mkdtemp(prefix='digest-module-review-', dir=ROOT / '.tmp'))
    for name in ['src', 'tests', 'fixtures']:
        (base / name).mkdir()
    source = args.source.read_bytes()
    copied = base / 'src/validation-under-review.rs'
    copied.write_bytes(source)
    tests = (ROOT / 'docs/review/digest-generation-module.rs').read_text()
    prefix, cases = tests.split('static SERIAL:', 1)
    library = prefix.replace('mod cancellation {', 'pub mod cancellation {').replace('mod validation;', 'pub mod validation;')
    library = library.replace('path::PathBuf, ', '').replace('use sha2::{Digest, Sha256};\n', '')
    (base / 'src/lib.rs').write_text(library)
    (base / 'tests/reviewer.rs').write_text('use std::{fs, path::PathBuf, sync::atomic::{AtomicUsize, Ordering}};\nuse sha2::{Digest, Sha256};\nuse digest_generation_review::{cancellation, validation};\nstatic SERIAL:' + cases)
    (base / 'Cargo.toml').write_text('[package]\nname="digest-generation-review"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[dependencies]\nanyhow="1"\nserde={version="1",features=["derive"]}\nsha2="0.10"\nlibc="=0.2.191"\n')
    env = dict(os.environ, REVIEW_FIXTURE_ROOT=str(base / 'fixtures'), CARGO_TARGET_DIR=str(ROOT / '.tmp/review-digest-module-target'))
    command = [str(ROOT / 'scripts/with-local-tools'), 'cargo', 'test', '--offline', '--manifest-path', str(base / 'Cargo.toml'), '--test', 'reviewer', '--', '--test-threads=1']
    result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True, timeout=90)
    report = dict(provenance='Independent integration tests compile the unchanged production validation module as a normal dependency, so implementation-owner unit tests are not substituted for reviewer tests. A deterministic cancellation stub supplies the module error boundary; this is not a daemon transport cancellation test.', sourcePath=str(args.source.resolve()), sourceSha256=hashlib.sha256(source).hexdigest(), productionModuleByteIdentical=copied.read_bytes() == source, reviewTestSha256=hashlib.sha256(tests.encode()).hexdigest(), workRoot=str(base), command=command, exitCode=result.returncode, stdout=result.stdout, stderr=result.stderr)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(result.stdout)
    if result.returncode:
        print(result.stderr)
        raise SystemExit(result.returncode)


if __name__ == '__main__':
    main()
