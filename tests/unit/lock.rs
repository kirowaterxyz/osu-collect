use osu_collect::download::lock::ActiveDownloadRegistry;
use std::path::PathBuf;

#[test]
fn registry_insert_and_remove() {
    let registry = ActiveDownloadRegistry::new();
    let path = PathBuf::from("/tmp/test-download-registry");
    assert!(registry.try_insert(&path));
    assert!(!registry.try_insert(&path));
    registry.remove(&path);
    assert!(registry.try_insert(&path));
    registry.remove(&path);
}

#[test]
fn registry_independent_paths() {
    let registry = ActiveDownloadRegistry::new();
    let path_a = PathBuf::from("/tmp/registry-a");
    let path_b = PathBuf::from("/tmp/registry-b");
    assert!(registry.try_insert(&path_a));
    assert!(registry.try_insert(&path_b));
    registry.remove(&path_a);
    registry.remove(&path_b);
}
