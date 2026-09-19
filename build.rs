fn main() {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let parts_dir = manifest_dir.join("src/ui/reader_parts");
    let mut names: Vec<_> = std::fs::read_dir(&parts_dir)
        .expect("reader_parts")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("txt"))
        .collect();
    names.sort();
    let mut out = String::new();
    for p in &names {
        out.push_str(&std::fs::read_to_string(p).unwrap_or_else(|e| panic!("read {:?}: {e}", p)));
    }
    let dest = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("reader_window.gen.rs");
    std::fs::write(&dest, out).unwrap();
    println!("cargo:rerun-if-changed=src/ui/reader_parts");
    for p in &names {
        println!("cargo:rerun-if-changed={}", p.display());
    }
}
