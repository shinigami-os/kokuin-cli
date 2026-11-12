//! Embeds the knowledge base (`kb/` at the repository root) into the crate.
//!
//! Generates `$OUT_DIR/embedded.rs` with one `include_str!` per article and rule
//! file, so the binary is self-contained and the files are still plain Markdown and
//! TOML that contributors edit directly.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    let kb_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../kb");
    let kb_dir = kb_dir.canonicalize().expect("the kb/ directory is missing");
    println!("cargo::rerun-if-changed={}", kb_dir.display());

    let articles_dir = kb_dir.join("articles");
    let mut articles = Vec::new();
    collect_markdown(&articles_dir, &mut articles);
    articles.sort();

    let mut out = String::from("/// `(id, Markdown source)` of every article.\n");
    out.push_str("pub(crate) static ARTICLES: &[(&str, &str)] = &[\n");
    for path in &articles {
        println!("cargo::rerun-if-changed={}", path.display());
        let id = article_id(&articles_dir, path);
        out.push_str(&format!(
            "    ({id:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    out.push_str("];\n");

    for (name, file) in [
        ("KMSG_RULES", "kmsg.toml"),
        ("PROCESS_RULES", "processes.toml"),
    ] {
        let path = kb_dir.join("rules").join(file);
        println!("cargo::rerun-if-changed={}", path.display());
        out.push_str(&format!(
            "pub(crate) static {name}: &str = include_str!({:?});\n",
            path.display().to_string()
        ));
    }

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("embedded.rs");
    fs::write(out_path, out).unwrap();
}

/// Recursively lists the `.md` files of `dir`.
fn collect_markdown(dir: &Path, found: &mut Vec<PathBuf>) {
    println!("cargo::rerun-if-changed={}", dir.display());
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_markdown(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            found.push(path);
        }
    }
}

/// `articles/firmware/fpdt.md` → `firmware.fpdt`.
fn article_id(articles_dir: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(articles_dir).unwrap().with_extension("");
    let parts: Vec<_> = relative
        .components()
        .map(|c| c.as_os_str().to_str().unwrap())
        .collect();
    parts.join(".")
}
