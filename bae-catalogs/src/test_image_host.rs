//! A local stand-in for the hosts provider images come from, and the image
//! bytes it serves, for the tests of this crate and every crate above it that
//! downloads covers or artist pictures.

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

/// The host a test's own image addresses are on. It resolves nowhere; only a
/// transport routed to [`RemoteImageHost::origin`] reaches it.
pub const TEST_IMAGE_HOST: &str = "images.bae.test";

/// A local stand-in for the hosts provider images come from: the Cover Art
/// Archive, and whatever address a test hands out for an image of its own.
///
/// One per test, started on that test's runtime and gone with it, and reached
/// only through the transport it is routed into. Cover addresses are derived
/// from a release's MusicBrainz ids rather than looked up, so any fixture whose
/// release document says the archive holds a front image sends the commit to
/// one of those addresses; this answers them. An address no test registered
/// answers 404 — the archive holding nothing there.
pub struct RemoteImageHost {
    routes: ImageRoutes,
    requests: ImageRequests,
    holds: ImageHolds,
    origin: String,
}

/// The status and body each registered path answers with.
type ImageRoutes =
    std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, (u16, Vec<u8>)>>>;

/// How many requests each path has had, registered or not.
type ImageRequests = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, usize>>>;

/// The paths whose answers wait for a test to let them go: each request says
/// it arrived, then waits until its hold is released.
type ImageHolds = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HeldPath>>>;

#[derive(Clone)]
struct HeldPath {
    arrived: std::sync::Arc<tokio::sync::Notify>,
    released: tokio::sync::watch::Receiver<bool>,
}

/// The state every request to the host reads.
#[derive(Clone)]
struct HostState {
    routes: ImageRoutes,
    requests: ImageRequests,
    holds: ImageHolds,
}

/// A path on a [`RemoteImageHost`] whose answers wait, from
/// [`RemoteImageHost::hold_image`]. Dropping it lets them go.
pub struct ImageHold {
    arrived: std::sync::Arc<tokio::sync::Notify>,
    release: tokio::sync::watch::Sender<bool>,
}

impl ImageHold {
    /// Wait until a request for the held path has reached the host.
    pub async fn arrived(&self) {
        self.arrived.notified().await;
    }

    /// Answer every request for the held path, waiting or still to come.
    pub fn release(self) {}
}

impl Drop for ImageHold {
    fn drop(&mut self) {
        self.release.send_replace(true);
    }
}

impl RemoteImageHost {
    pub async fn start() -> Self {
        use axum::extract::{Request, State};
        use axum::http::StatusCode;

        async fn handler(
            State(HostState { routes, requests, holds }): State<HostState>,
            request: Request,
        ) -> (StatusCode, [(axum::http::HeaderName, &'static str); 1], Vec<u8>) {
            *requests
                .lock()
                .expect("image host requests mutex poisoned")
                .entry(request.uri().path().to_string())
                .or_default() += 1;
            let held = holds
                .lock()
                .expect("image host holds mutex poisoned")
                .get(request.uri().path())
                .cloned();
            if let Some(HeldPath { arrived, mut released }) = held {
                arrived.notify_one();
                released
                    .wait_for(|released| *released)
                    .await
                    .expect("a hold says it is released before it goes");
            }
            let answer = routes
                .lock()
                .expect("image host routes mutex poisoned")
                .get(request.uri().path())
                .cloned();
            let (status, bytes) = answer.unwrap_or((404, Vec::new()));
            (
                StatusCode::from_u16(status).expect("a valid stub status"),
                [(axum::http::header::CONTENT_TYPE, "image/png")],
                bytes,
            )
        }

        let routes: ImageRoutes = std::sync::Arc::default();
        let requests: ImageRequests = std::sync::Arc::default();
        let holds: ImageHolds = std::sync::Arc::default();
        let listener =
            tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("the test image host binds");
        let origin = format!(
            "http://{}",
            listener.local_addr().expect("the test image host has an address")
        );
        let app = axum::Router::new().fallback(handler).with_state(HostState {
            routes: routes.clone(),
            requests: requests.clone(),
            holds: holds.clone(),
        });
        #[expect(
            clippy::disallowed_methods,
            reason = "test support: the host serves until its test ends"
        )]
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("the test image host serves until the test ends");
        });
        Self { routes, requests, holds, origin }
    }

    /// Where this host listens, for a test that routes only some hosts here
    /// rather than every host [`Self::route`] sends.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// `http` with the Cover Art Archive and this test's own image host sent
    /// here.
    pub fn route(&self, http: bae_util::http::Http) -> bae_util::http::Http {
        http.serve("coverartarchive.org", &self.origin).serve(TEST_IMAGE_HOST, &self.origin)
    }

    /// Serve `bytes` as a MusicBrainz release's front image, at the original's
    /// address and every downscaled copy's.
    pub fn serve_front(&self, release_id: &str, bytes: Vec<u8>) {
        self.answer_front(release_id, 200, bytes);
    }

    /// Answer `status` for a release's front image, for a test driving the
    /// download's failure path.
    pub fn fail_front(&self, release_id: &str, status: u16) {
        self.answer_front(release_id, status, Vec::new());
    }

    /// Serve image bytes at `path` on this test's image host and return the
    /// address.
    pub fn serve_image(&self, path: &str, bytes: Vec<u8>) -> String {
        assert!(path.starts_with('/'), "test image path must be absolute");
        self.routes
            .lock()
            .expect("image host routes mutex poisoned")
            .insert(path.to_string(), (200, bytes));
        format!("https://{TEST_IMAGE_HOST}{path}")
    }

    /// Answer `status` with no body at `path` on this test's image host, for
    /// a test driving a download's failure path, and return the address.
    pub fn fail_image(&self, path: &str, status: u16) -> String {
        assert!(path.starts_with('/'), "test image path must be absolute");
        self.routes
            .lock()
            .expect("image host routes mutex poisoned")
            .insert(path.to_string(), (status, Vec::new()));
        format!("https://{TEST_IMAGE_HOST}{path}")
    }

    /// Hold every answer at `path` until the returned hold is released, for
    /// a test that looks at what a caller does while it waits on the image.
    pub fn hold_image(&self, path: &str) -> ImageHold {
        assert!(path.starts_with('/'), "test image path must be absolute");
        let arrived = std::sync::Arc::new(tokio::sync::Notify::new());
        let (release, released) = tokio::sync::watch::channel(false);
        self.holds
            .lock()
            .expect("image host holds mutex poisoned")
            .insert(path.to_string(), HeldPath { arrived: arrived.clone(), released });
        ImageHold { arrived, release }
    }

    /// How many requests `path` has had on this host.
    pub fn requests(&self, path: &str) -> usize {
        self.requests
            .lock()
            .expect("image host requests mutex poisoned")
            .get(path)
            .copied()
            .unwrap_or(0)
    }

    fn answer_front(&self, release_id: &str, status: u16, bytes: Vec<u8>) {
        let mut routes = self.routes.lock().expect("image host routes mutex poisoned");
        for suffix in ["front", "front-250", "front-500", "front-1200"] {
            routes.insert(format!("/release/{release_id}/{suffix}"), (status, bytes.clone()));
        }
    }
}
