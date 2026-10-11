//! Discovery assertions ported from all ten main:test/config.test.ts cases.
use psst::config::discover;
use std::{fs, path::PathBuf};
struct Tree {
    _temp: tempfile::TempDir,
    home: PathBuf,
    parent: PathBuf,
    cwd: PathBuf,
}
impl Tree {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let parent = home.join("projects");
        let cwd = parent.join("project");
        fs::create_dir_all(&cwd).unwrap();
        Self {
            _temp: temp,
            home,
            parent,
            cwd,
        }
    }
    fn find(&self) -> PathBuf {
        discover(&self.cwd, Some(&self.home), None)
    }
}
#[test]
fn override_absolute_even_when_missing() {
    let tree = Tree::new();
    let path = tree._temp.path().join("custom/config.json");
    assert_eq!(
        discover(&tree.cwd, Some(&tree.home), Some(path.as_os_str())),
        path
    );
}
#[test]
fn hidden_config_in_current_directory() {
    let tree = Tree::new();
    let path = tree.cwd.join(".psst.json");
    fs::write(&path, "{}").unwrap();
    assert_eq!(tree.find(), path);
}
#[test]
fn visible_config_in_current_directory() {
    let tree = Tree::new();
    let path = tree.cwd.join("psst.json");
    fs::write(&path, "{}").unwrap();
    assert_eq!(tree.find(), path);
}
#[test]
fn visible_filename_wins_in_same_directory() {
    // The old test's title says hidden wins, but its assertion says psst.json wins.
    let tree = Tree::new();
    fs::write(tree.cwd.join(".psst.json"), "{}").unwrap();
    let path = tree.cwd.join("psst.json");
    fs::write(&path, "{}").unwrap();
    assert_eq!(tree.find(), path);
}
#[test]
fn parent_config() {
    let tree = Tree::new();
    let path = tree.parent.join(".psst.json");
    fs::write(&path, "{}").unwrap();
    assert_eq!(tree.find(), path);
}
#[test]
fn home_config() {
    let tree = Tree::new();
    let path = tree.home.join(".psst.json");
    fs::write(&path, "{}").unwrap();
    assert_eq!(tree.find(), path);
}
#[test]
fn no_config_falls_back_to_visible_cwd_path() {
    let tree = Tree::new();
    assert_eq!(tree.find(), tree.cwd.join("psst.json"));
    // A config above home must not be inherited.
    fs::write(tree._temp.path().join("psst.json"), "{}").unwrap();
    assert_eq!(tree.find(), tree.cwd.join("psst.json"));
}
#[test]
fn nearest_config_wins_over_parent_and_home() {
    let tree = Tree::new();
    fs::write(tree.parent.join("psst.json"), "{}").unwrap();
    fs::write(tree.home.join(".psst.json"), "{}").unwrap();
    let path = tree.cwd.join(".psst.json");
    fs::write(&path, "{}").unwrap();
    assert_eq!(tree.find(), path);
}
// main duplicates the parent and home cases with these "when not in ..." titles.
#[test]
fn parent_when_current_missing() {
    parent_config();
}
#[test]
fn home_when_current_and_parent_missing() {
    home_config();
}
