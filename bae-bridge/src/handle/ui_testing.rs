use super::*;

forward! {
    #[cfg(debug_assertions)]
    async this => {
        /// Write the library fixture a debug UI test left at `fixture_path`
        /// into the open library: the albums it names, each with its artist,
        /// one release, and its tracks.
        fn write_library_fixture(fixture_path: String) -> () {
            let fixture =
                bae_core::library::LibraryFixture::read(std::path::Path::new(&fixture_path))
                    .map_err(BridgeError::internal)?;
            this.services
                .write_fixture(&fixture)
                .await
                .map_err(BridgeError::internal)
        }
    }
}
