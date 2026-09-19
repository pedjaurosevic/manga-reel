//! Import selected paths (or migrate existing links) without opening the GUI.
#![allow(dead_code)]
#[path = "../src/archive.rs"]
mod archive;
#[path = "../src/library.rs"]
mod library;
fn main() -> anyhow::Result<()> {
    let mut state = library::load_state();
    let args: Vec<_> = std::env::args().skip(1).collect();
    let paths = if args == ["--linked"] {
        state
            .files
            .iter()
            .filter(|p| !library::is_managed(p))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        args.iter().map(std::path::PathBuf::from).collect()
    };
    for source in paths {
        let destination = library::import_into(&source, &library::books_dir(), |_, _| {})?;
        library::register_import(&mut state, &source, destination.clone())?;
        library::save_state(&state)?;
        let cover = library::ensure_cover(&destination)?;
        println!(
            "Imported {} ({} bytes), cover {}",
            destination.file_name().unwrap().to_string_lossy(),
            std::fs::metadata(&destination)?.len(),
            cover.display()
        );
    }
    Ok(())
}
