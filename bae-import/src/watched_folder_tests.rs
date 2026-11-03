use super::*;

#[test]
fn root_path_uses_the_full_path_as_name() {
    let folder = WatchedFolder::from_path(PathBuf::from("/"));
    assert_eq!(folder.name, "/");
}
