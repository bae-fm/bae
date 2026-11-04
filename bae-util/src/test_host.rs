//! A local HTTP host a test stands in for remote services with. Routes match
//! by substring of the request target, in the order added; every request is
//! recorded, and a test can hold requests until it lets them through.

use std::sync::{Arc, Mutex};

/// The local host. Requests reach it through [`crate::http::Http::serve`]
/// pointed at [`FakeHost::base_url`].
pub struct FakeHost {
    base_url: String,
    state: Arc<Mutex<FakeState>>,
    /// How many requests have arrived, told as each is recorded.
    arrivals: Arc<tokio::sync::watch::Sender<usize>>,
}

#[derive(Default)]
struct FakeState {
    routes: Vec<(String, u16, Vec<u8>)>,
    requests: Vec<String>,
    /// While set, a request containing any of the needles waits here after
    /// recording itself.
    gate: Option<(Vec<String>, Arc<tokio::sync::Semaphore>)>,
}

impl FakeHost {
    /// Bind a port on the loopback address and answer every connection to it.
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("fake host binds");
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(FakeState::default()));
        let arrivals = Arc::new(tokio::sync::watch::channel(0).0);
        let accept_state = state.clone();
        let accept_arrivals = arrivals.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let state = accept_state.clone();
                let arrivals = accept_arrivals.clone();
                tokio::spawn(async move { serve_one(stream, state, &arrivals).await });
            }
        });
        FakeHost { base_url, state, arrivals }
    }

    /// The origin this host answers on.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Answer any request containing `needle` with `status` and `body`; an
    /// earlier route wins over a later one.
    pub fn route(&self, needle: &str, status: u16, body: impl Into<Vec<u8>>) {
        self.state.lock().unwrap().routes.push((needle.to_string(), status, body.into()));
    }

    /// Answer requests containing `needle` with `status` and `body` from now
    /// on, in place of what the route for it said before.
    pub fn reroute(&self, needle: &str, status: u16, body: impl Into<Vec<u8>>) {
        let mut state = self.state.lock().unwrap();
        state.routes.retain(|(existing, _, _)| existing != needle);
        state.routes.insert(0, (needle.to_string(), status, body.into()));
    }

    /// Replace every route.
    pub fn set_routes(&self, routes: Vec<(&str, u16, String)>) {
        let mut state = self.state.lock().unwrap();
        state.routes = routes
            .into_iter()
            .map(|(needle, status, body)| (needle.to_string(), status, body.into_bytes()))
            .collect();
    }

    /// Leave every request matching `needle` unanswered until
    /// [`Self::release`], so a test can act on a request in flight.
    pub fn hold(&self, needle: &str) {
        self.hold_each(&[needle]);
    }

    /// [`Self::hold`] every request matching any of `needles`.
    pub fn hold_each(&self, needles: &[&str]) {
        self.state.lock().unwrap().gate = Some((
            needles.iter().map(|needle| needle.to_string()).collect(),
            Arc::new(tokio::sync::Semaphore::new(0)),
        ));
    }

    /// Answer one held request, the next to reach the hold if none is
    /// waiting, and go on holding the rest.
    pub fn let_one_through(&self) {
        if let Some((_, gate)) = self.state.lock().unwrap().gate.as_ref() {
            gate.add_permits(1);
        }
    }

    /// Answer everything held, and let later requests through.
    pub fn release(&self) {
        if let Some((_, gate)) = self.state.lock().unwrap().gate.take() {
            gate.close();
        }
    }

    /// Every request target received so far, in arrival order.
    pub fn requests(&self) -> Vec<String> {
        self.state.lock().unwrap().requests.clone()
    }

    /// How many requests so far had a target containing `needle`.
    pub fn count_containing(&self, needle: &str) -> usize {
        self.requests().iter().filter(|target| target.contains(needle)).count()
    }

    /// Wait until `count` requests containing `needle` have arrived, failing
    /// the test if they never do.
    pub async fn wait_for_request(&self, needle: &str, count: usize) {
        let mut arrivals = self.arrivals.subscribe();
        if tokio::time::timeout(
            crate::test_channels::HANG_GUARD,
            arrivals.wait_for(|_| self.count_containing(needle) >= count),
        )
        .await
        .is_err()
        {
            panic!(
                "the host received no request containing {needle:?} (wanted {count}); \
                 requests so far: {:?}",
                self.requests()
            );
        }
    }
}

async fn serve_one(
    mut stream: tokio::net::TcpStream,
    state: Arc<Mutex<FakeState>>,
    arrivals: &tokio::sync::watch::Sender<usize>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    // One request per connection: every response says `Connection: close`, so
    // headers always arrive in the first read or two and there is no pipelining
    // to unpick.
    while !buffer.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    }
    let head = String::from_utf8_lossy(&buffer).to_string();
    let target = head.split_whitespace().nth(1).unwrap_or_default().to_string();

    let (status, body, gate) = {
        let mut state = state.lock().unwrap();
        state.requests.push(target.clone());
        let (status, body) = state
            .routes
            .iter()
            .find(|(needle, _, _)| target.contains(needle.as_str()))
            .map(|(_, status, body)| (*status, body.clone()))
            .unwrap_or((404, b"{}".to_vec()));
        let gate = state
            .gate
            .as_ref()
            .filter(|(needles, _)| needles.iter().any(|needle| target.contains(needle.as_str())))
            .map(|(_, gate)| gate.clone());
        (status, body, gate)
    };
    arrivals.send_modify(|arrived| *arrived += 1);
    if let Some(gate) = gate {
        // The hold ends by closing the semaphore, which ends every wait, so
        // no release can be missed; a permit lets one request through and is
        // spent on it.
        if let Ok(permit) = gate.acquire().await {
            permit.forget();
        }
    }

    let response_head = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a client that stopped reading needs no answer"
    )]
    let _ = stream.write_all(&[response_head.as_bytes(), &body].concat()).await;
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a client that already closed needs no shutdown"
    )]
    let _ = stream.shutdown().await;
}
