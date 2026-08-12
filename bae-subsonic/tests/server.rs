//! Integration tests driving the real Subsonic router against a seeded bae
//! library. Each test states the spec behavior it checks; the library is built
//! through the ordinary import path (a per-track release and a single-file CUE
//! release), so the assertions run against real rows, audio, and cover blobs.

include!("server/library_and_browse.rs");
include!("server/streaming.rs");
