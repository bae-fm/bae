use super::*;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn tree() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    write(r, "bae-shots/canvas/dc-runtime.js", "runtime");
    write(r, TOKENS, "tokens");
    write(r, BUNDLE_CSS, "css");
    write(r, BUNDLE_JS, "js");
    write(r, "design/mocks/mac.a.b.dc.html", "<x-dc>board</x-dc>");
    write(r, "secret.txt", "secret");
    root
}

async fn get(server: &Server, path: &str) -> String {
    let mut stream = TcpStream::connect(server.address).await.unwrap();
    stream.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

#[test]
fn routes_cover_the_staged_tree() {
    let root = tree();
    let r = root.path();
    assert_eq!(route(r, "/support.js"), Some(runtime::copy_path(r)));
    assert_eq!(route(r, "/storage/support.js"), Some(runtime::copy_path(r)));
    assert_eq!(route(r, "/ds/bae/tokens.css"), Some(r.join(TOKENS)));
    assert_eq!(route(r, "/storage/ds/bae/components/bundle.js"), Some(r.join(BUNDLE_JS)));
    assert_eq!(route(r, "/mac.a.b.dc.html?x=1"), Some(r.join("design/mocks/mac.a.b.dc.html")));
    assert_eq!(route(r, "/nothing.dc.html"), None);
    assert_eq!(route(r, "/"), None);
}

#[test]
fn routes_stay_inside_the_mocks() {
    let root = tree();
    assert_eq!(route(root.path(), "/../../secret.txt"), None);
    assert_eq!(route(root.path(), "//etc/passwd"), None);
}

#[tokio::test]
async fn serves_files_and_refuses_the_rest() {
    let root = tree();
    with_server(root.path(), async |server| {
        let board = get(server, "/mac.a.b.dc.html").await;
        assert!(board.starts_with("HTTP/1.1 200 OK"), "{board}");
        assert!(board.contains("text/html"), "{board}");
        assert!(board.ends_with("<x-dc>board</x-dc>"), "{board}");
        assert!(get(server, "/support.js").await.ends_with("runtime"));
        assert!(get(server, "/missing.html").await.starts_with("HTTP/1.1 404"));
        assert!(server.board_url("mac.a.b").ends_with("/mac.a.b.dc.html"));
    })
    .await
    .unwrap();
}
