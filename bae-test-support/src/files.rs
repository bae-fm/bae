//! Audio files and images on disk, and the local stand-in for the Cover Art
//! Archive that serves the ones a release's document points at.

/// The FLAC fixture tree lives in bae-core, so it is reached relative to the
/// workspace root — `CARGO_MANIFEST_DIR` here is this crate, not bae-core.
fn bae_core_fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("bae-test-support sits one level under the workspace root")
        .join("bae-core/tests/fixtures")
}

/// Write one tagged FLAC into `dir` (copied from the test fixture) with the
/// given `title`, so an Unknown-identity import can map it from file tags.
/// Returns the on-disk bytes after tagging.
pub fn write_tagged_flac(dir: &std::path::Path, filename: &str, title: &str) -> Vec<u8> {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};

    let fixture = bae_core_fixtures().join("flac/01 Test Track 1.flac");
    let flac = std::fs::read(&fixture).expect("FLAC fixture missing");

    let dest = dir.join(filename);
    std::fs::write(&dest, &flac).unwrap();
    let mut tagged = lofty::read_from_path(&dest).expect("read for tagging");
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title(title.to_string());
    tag.set_artist("Artist Name".to_string());
    tag.set_album("Album Title".to_string());
    tag.insert_text(ItemKey::AlbumArtist, "Artist Name".to_string());
    tag.set_track(1);
    tagged.insert_tag(tag);
    tagged
        .save_to_path(&dest, WriteOptions::default())
        .expect("write tags");

    std::fs::read(&dest).unwrap()
}

/// Copy `source` into `dest_dir/name`, stamp Vorbis-comment tags on it (title,
/// artist, album, album artist, year, track), and return the destination path.
/// For tests that need a tagged audio file derived from a specific fixture.
pub fn copy_and_tag(
    source: &std::path::Path,
    dest_dir: &std::path::Path,
    name: &str,
    title: &str,
    artist: &str,
    album_title: &str,
    album_artist: &str,
    year: u16,
    track: u32,
) -> std::path::PathBuf {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::items::Timestamp;
    use lofty::tag::{Tag, TagType};

    let dest = dest_dir.join(name);
    std::fs::copy(source, &dest).expect("copy fixture");

    let mut tagged = lofty::read_from_path(&dest).expect("read for tagging");
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title(title.to_string());
    tag.set_artist(artist.to_string());
    tag.set_album(album_title.to_string());
    tag.insert_text(ItemKey::AlbumArtist, album_artist.to_string());
    tag.set_date(Timestamp {
        year,
        month: None,
        day: None,
        hour: None,
        minute: None,
        second: None,
    });
    tag.set_track(track);
    tagged.insert_tag(tag);
    tagged
        .save_to_path(&dest, WriteOptions::default())
        .expect("save tags");
    dest
}

/// Write a small solid-color PNG to `path` — a folder cover an import picks up,
/// so a test release actually has art (for exercising cover-embedding paths).
pub fn write_cover_png(path: &std::path::Path) {
    std::fs::write(path, cover_png()).expect("write cover png");
}

/// A small solid-color PNG, big enough to clear the download path's 100-byte
/// floor — the bytes a stand-in image host serves.
pub fn cover_png() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(16, 16, image::Rgb([90, 30, 160]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("encode cover png");
    bytes.into_inner()
}

/// A local stand-in for the Cover Art Archive, shared by every test in a
/// binary.
///
/// Cover addresses are derived from a release's MusicBrainz ids rather than
/// looked up, so any fixture whose release document says the archive holds a
/// front image sends the commit to one of those addresses. This answers them on
/// localhost. An address no test registered answers 404 — the archive holding
/// nothing there, which is both what an unregistered release means and what
/// keeps the rest of the suite from reaching the real service.
pub struct RemoteImageHost {
    routes: std::sync::Mutex<std::collections::HashMap<String, (u16, Vec<u8>)>>,
    base_url: std::sync::OnceLock<String>,
}

impl RemoteImageHost {
    /// Serve `bytes` as a MusicBrainz release's front image, at both the full
    /// and the thumbnail address.
    pub fn serve_front(&self, release_id: &str, bytes: Vec<u8>) {
        self.answer_front(release_id, 200, bytes);
    }

    /// Answer `status` for a release's front image, for a test driving the
    /// download's failure path.
    pub fn fail_front(&self, release_id: &str, status: u16) {
        self.answer_front(release_id, status, Vec::new());
    }

    /// Serve image bytes at an arbitrary provider URL path and return its URL.
    pub fn serve_image(&self, path: &str, bytes: Vec<u8>) -> String {
        assert!(path.starts_with('/'), "test image path must be absolute");
        self.routes
            .lock()
            .expect("image host routes mutex poisoned")
            .insert(path.to_string(), (200, bytes));
        format!(
            "{}{}",
            self.base_url.get().expect("test image host has started"),
            path
        )
    }

    fn answer_front(&self, release_id: &str, status: u16, bytes: Vec<u8>) {
        let mut routes = self.routes.lock().expect("archive routes mutex poisoned");
        for suffix in ["front", "front-250"] {
            routes.insert(
                format!("/release/{release_id}/{suffix}"),
                (status, bytes.clone()),
            );
        }
    }
}

/// The binary's stand-in archive, started and pointed at on first use.
pub fn cover_art_archive() -> &'static RemoteImageHost {
    static ARCHIVE: std::sync::OnceLock<&'static RemoteImageHost> = std::sync::OnceLock::new();
    ARCHIVE.get_or_init(start_cover_art_archive)
}

fn start_cover_art_archive() -> &'static RemoteImageHost {
    use axum::extract::{Request, State};
    use axum::http::StatusCode;

    let archive: &'static RemoteImageHost = Box::leak(Box::new(RemoteImageHost {
        routes: std::sync::Mutex::new(std::collections::HashMap::new()),
        base_url: std::sync::OnceLock::new(),
    }));

    async fn handler(
        State(archive): State<&'static RemoteImageHost>,
        request: Request,
    ) -> (
        StatusCode,
        [(axum::http::HeaderName, &'static str); 1],
        Vec<u8>,
    ) {
        let answer = archive
            .routes
            .lock()
            .expect("archive routes mutex poisoned")
            .get(request.uri().path())
            .cloned();
        let (status, bytes) = answer.unwrap_or((404, Vec::new()));
        (
            StatusCode::from_u16(status).expect("a valid stub status"),
            [(axum::http::header::CONTENT_TYPE, "image/png")],
            bytes,
        )
    }

    // Its own runtime on its own thread: the archive outlives each `#[tokio::test]`
    // that reaches it, so it cannot live on any one test's runtime.
    let (address_tx, address_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the stub archive's runtime builds");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("the stub archive binds");
            address_tx
                .send(
                    listener
                        .local_addr()
                        .expect("the stub archive has an address"),
                )
                .expect("the starting thread is waiting for the address");
            let app = axum::Router::new().fallback(handler).with_state(archive);
            let _ = axum::serve(listener, app).await;
        });
    });

    let address = address_rx.recv().expect("the stub archive starts");
    let base_url = format!("http://{address}");
    archive
        .base_url
        .set(base_url.clone())
        .expect("the test image host URL is set once");
    bae_core::import::cover_art::set_base_url_for_test(Some(base_url));
    archive
}

pub async fn read_cover_image_blob(
    mgr: &bae_core::library::LibraryManager,
    release_id: &str,
) -> Option<Vec<u8>> {
    mgr.read_cover_image_blob(release_id).await.unwrap()
}
