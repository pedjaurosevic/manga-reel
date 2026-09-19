fn main() {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let parts_dir = manifest_dir.join("src/ui/reader_parts");
    let mut names: Vec<_> = std::fs::read_dir(&parts_dir)
        .expect("reader_parts")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.ends_with(".txt.z64"))
                .unwrap_or(false)
        })
        .collect();
    names.sort();

    // Prefer python3 inflate (always available on build hosts); no extra crate deps.
    let script = r#"
import base64, sys, zlib
from pathlib import Path
parts = sorted(Path(sys.argv[1]).glob('p*.txt.z64'))
out = bytearray()
for p in parts:
    out.extend(zlib.decompress(base64.b64decode(p.read_text().encode())))
sys.stdout.buffer.write(out)
"#;

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let dest = out_dir.join("reader_window.gen.rs");

    let mut child = Command::new("python3")
        .arg("-c")
        .arg(script)
        .arg(&parts_dir)
        .stdout(Stdio::piped())
        .spawn()
        .expect("python3 required to assemble reader_window from reader_parts");
    let mut decoded = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut decoded)
        .expect("read python stdout");
    let status = child.wait().expect("wait python");
    assert!(status.success(), "python inflate failed: {status}");
    std::fs::write(&dest, decoded).expect("write reader_window.gen.rs");

    println!("cargo:rerun-if-changed=src/ui/reader_parts");
    for p in &names {
        println!("cargo:rerun-if-changed={}", p.display());
    }
}
