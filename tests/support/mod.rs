pub mod agent;

#[allow(dead_code)]
pub fn tempdir() -> tempfile::TempDir {
    // Keep destination fixtures free of macOS temporary-root symlinks.
    let root = std::env::temp_dir().canonicalize().unwrap();
    tempfile::tempdir_in(root).unwrap()
}
