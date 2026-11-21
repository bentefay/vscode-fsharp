use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
};

fn source_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("read native source directory") {
        let entry = entry.expect("read native source entry");
        let path = entry.path();
        if path.is_dir() {
            source_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn grammar_files(directory: &Path, files: &mut Vec<PathBuf>) {
    println!("cargo:rerun-if-changed={}", directory.display());
    for entry in fs::read_dir(directory).expect("read vendored grammar directory") {
        let entry = entry.expect("read vendored grammar entry");
        let kind = entry.file_type().expect("read vendored grammar file kind");
        if kind.is_dir() {
            grammar_files(&entry.path(), files);
        } else if kind.is_file() {
            files.push(entry.path());
        } else {
            panic!("Vendored grammar input must be a regular file or directory");
        }
    }
}

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("crate directory"));
    let workspace = root.parent().unwrap().parent().unwrap();
    let mut files = Vec::new();
    source_files(&root.join("src"), &mut files);
    files.extend([
        root.join("build.rs"),
        root.join("Cargo.toml"),
        workspace.join("Cargo.toml"),
        workspace.join("Cargo.lock"),
    ]);
    let grammar = workspace.join("vendor/tree-sitter-fsharp");
    println!("cargo:rerun-if-changed={}", grammar.display());
    if grammar.exists() {
        // The parser, scanner, generated tables, headers and grammar sources
        // are semantic inputs even when their Cargo version is unchanged.
        grammar_files(&grammar, &mut files);
    }
    files.sort();
    // Every semantic source participates, including modules added later. Hash
    // during compilation: the daemon embeds only a 64-byte identity, not all
    // engine source literals or a startup hashing workload.
    let mut fingerprint = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    for file in files {
        println!("cargo:rerun-if-changed={}", file.display());
        let relative = file
            .strip_prefix(workspace)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        fingerprint.update(relative.as_bytes());
        fingerprint.update(&[0]);
        // Generated parser tables are large; hashing must not retain a second
        // complete parser copy in the build process.
        let mut input = fs::File::open(&file).expect("open engine fingerprint source");
        loop {
            let count = input
                .read(&mut buffer)
                .expect("read engine fingerprint source");
            if count == 0 {
                break;
            }
            fingerprint.update(&buffer[..count]);
        }
        fingerprint.update(&[0]);
    }
    println!("cargo:rerun-if-changed=src");
    println!(
        "cargo:rustc-env=FSHARP_NAVIGATOR_ENGINE_FINGERPRINT={}",
        fingerprint.finalize().to_hex()
    );
}
