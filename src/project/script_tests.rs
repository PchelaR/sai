//! Scripts that delete on the VPS, run for real: they must refuse anything outside the
//! project. Linux only
use std::fs;

use super::files::REMOVE_SKIPPED;
use crate::testing::{self, TempDir, fails, ok};

#[test]
fn removes_skipped_items_only_inside_the_project() {
    let home = TempDir::new("skipped");
    let root = home.join("projects/shop/front");
    for file in ["data/raw/a.csv", "keep.txt", "logs/x.log"] {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }
    let root_text = root.to_str().unwrap();
    ok(testing::run(
        REMOVE_SKIPPED,
        &[root_text, "data/raw", "logs/x.log"],
        &home.path,
    ));
    assert!(!root.join("data/raw").exists());
    assert!(root.join("data").exists(), "only what was named");
    assert!(!root.join("logs/x.log").exists());
    assert!(root.join("keep.txt").exists());

    for bad in ["../front", "a/../../x", "./keep.txt", "/etc"] {
        fails(testing::run(REMOVE_SKIPPED, &[root_text, bad], &home.path));
    }
    let outside = home.join("elsewhere");
    fs::create_dir_all(outside.join("x")).unwrap();
    fails(testing::run(
        REMOVE_SKIPPED,
        &[outside.to_str().unwrap(), "x"],
        &home.path,
    ));
    assert!(outside.join("x").exists());
    assert!(root.join("keep.txt").exists());
}
