#!/usr/bin/env python3
"""Capture native build inputs once, verify stability, and archive provenance.

Generated grammar tables are essential build inputs. Never reconstruct a stage
from Rust files alone when a local grammar dependency is present.
"""
import argparse
import hashlib
import json
import pathlib
import shutil
import tarfile
import tempfile


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def inputs(root):
    paths = {root / "Cargo.toml", root / "Cargo.lock"}
    paths.update(root.glob("crates/**/*.rs"))
    paths.update(root.glob("crates/**/Cargo.toml"))
    grammar = root / "vendor/tree-sitter-fsharp"
    if grammar.exists():
        for path in grammar.rglob("*"):
            if path.is_symlink():
                raise RuntimeError(f"Symlink is not a captured grammar input: {path}")
            if path.is_file():
                paths.add(path)
    for path in paths:
        if path.is_symlink() or not path.is_file():
            raise RuntimeError(f"Build input must be a regular file: {path}")
    return sorted(paths)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=pathlib.Path,
                        default=pathlib.Path(__file__).resolve().parents[1])
    parser.add_argument("--selection", required=True)
    parser.add_argument("--helper-sha")
    parser.add_argument("--limitation", action="append", default=[])
    options = parser.parse_args()
    root = options.root.resolve()
    runtime = root / "evaluation/.runtime"
    runtime.mkdir(parents=True, exist_ok=True)
    stage = pathlib.Path(tempfile.mkdtemp(prefix="source-capture-", dir=runtime))
    files = inputs(root)
    hashes = {}
    try:
        for path in files:
            relative = path.relative_to(root)
            target = stage / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
            hashes[relative.as_posix()] = digest(target)
        if inputs(root) != files or any(digest(root / p) != h for p, h in hashes.items()):
            raise RuntimeError("Native inputs changed during capture; retry at a stable boundary")
        canonical = json.dumps(hashes, sort_keys=True, separators=(",", ":")).encode()
        capture = hashlib.sha256(canonical).hexdigest()
        provenance = {
            "captureSha256": capture,
            "sourceSha256": hashes,
            "selection": options.selection,
            "knownLimitations": options.limitation,
        }
        if options.helper_sha:
            provenance["matchingHelperSha256"] = options.helper_sha
        (stage / "source-provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
        destination = runtime / ("source-stage-" + capture[:12])
        if destination.exists():
            raise RuntimeError(f"Immutable stage already exists: {destination}")
        stage.rename(destination)
        stage = destination
        candidates = root / "evaluation/candidates"
        candidates.mkdir(parents=True, exist_ok=True)
        base = candidates / ("source-" + capture[:12])
        if base.with_suffix(".json").exists() or base.with_suffix(".tar.gz").exists():
            raise RuntimeError(f"Immutable source evidence already exists: {base}")
        base.with_suffix(".json").write_text(json.dumps(provenance, indent=2) + "\n")
        with tarfile.open(base.with_suffix(".tar.gz"), "w:gz") as archive:
            for path in sorted([*hashes, "source-provenance.json"]):
                archive.add(stage / path, arcname=path)
        print(json.dumps({"captureSha256": capture, "stage": str(stage),
                          "sourceFiles": len(hashes), "archive": str(base.with_suffix(".tar.gz"))}))
    except Exception:
        # Preserve a completed immutable stage if archival alone failed.
        if stage.name.startswith("source-capture-"):
            shutil.rmtree(stage)
        raise


if __name__ == "__main__":
    main()
