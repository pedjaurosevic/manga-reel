use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let b64_path = manifest.join("src/ui/reader_window.rs.gz.b64");
    println!("cargo:rerun-if-changed={}", b64_path.display());

    let b64 = fs::read_to_string(&b64_path).expect("read reader blob");
    let b64: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
    use base64::Engine;
    let gz = base64::engine::general_purpose::STANDARD
        .decode(b64.as_bytes())
        .expect("b64 decode");
    let mut decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(gz));
    let mut rust_src = String::new();
    std::io::Read::read_to_string(&mut decoder, &mut rust_src).expect("gunzip");

    let gen = out.join("reader_window.gen.rs");
    let mut f = fs::File::create(&gen).expect("create gen");
    f.write_all(rust_src.as_bytes()).expect("write gen");
}
