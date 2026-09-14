/// Lists folders as the OS does, except that the second release folder the
/// walk asks for stays closed until the test opens it. Which folder that is
/// depends on the walk's own order, which this test does not assume: it
/// records the first one and holds whichever comes next.
struct HeldListing {
    first: std::sync::Mutex<Option<PathBuf>>,
    entered: tokio::sync::mpsc::UnboundedSender<PathBuf>,
    gate: std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
}

impl HeldListing {
    fn new() -> (Self, tokio::sync::mpsc::UnboundedReceiver<PathBuf>) {
        let (entered, entered_rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Self {
                first: std::sync::Mutex::new(None),
                entered,
                gate: std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new())),
            },
            entered_rx,
        )
    }

    fn open(&self) {
        let (lock, condition) = &*self.gate;
        *lock.lock().unwrap() = true;
        condition.notify_all();
    }
}

/// Opens the gate when dropped, so a failed assertion releases the parked
/// walk and the test reports the failure instead of waiting on the walk at
/// runtime shutdown.
struct OpensOnDrop<'a>(&'a HeldListing);

impl Drop for OpensOnDrop<'_> {
    fn drop(&mut self) {
        self.0.open();
    }
}

impl crate::import::folder_scanner::DirectoryReader for HeldListing {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &crate::import::folder_scanner::ScanCancellation,
    ) -> Result<crate::import::folder_scanner::DirectoryListing, crate::import::folder_scanner::FolderScanError>
    {
        if directory.components().count() == 1 {
            // Decided under the lock and waited on outside it: the test reads
            // `first` while this holds the gate.
            let hold = {
                let mut first = self.first.lock().unwrap();
                match &*first {
                    None => {
                        *first = Some(directory.to_path_buf());
                        false
                    }
                    Some(first) => first != directory,
                }
            };
            if hold {
                self.entered
                    .send(directory.to_path_buf())
                    .expect("the test waits on the held folder");
                let (lock, condition) = &*self.gate;
                let mut open = lock.lock().unwrap();
                while !*open {
                    open = condition.wait(open).unwrap();
                }
            }
        }
        crate::import::folder_scanner::OsDirectoryReader.read(root, directory, cancellation)
    }
}

/// A scan announces each candidate as the walk finds it, not as one batch
/// once the walk ends. Walk duration scales with the tree and with how fast
/// the volume answers — a network share is orders of magnitude slower than a
/// local disk — and a list that stays empty for that whole span is
/// indistinguishable from a scan that found nothing. This holds one folder's
/// listing closed and requires the folder read before it to be announced
/// meanwhile, whatever the walk's order and however it is built.
#[tokio::test]
async fn a_candidate_is_announced_while_the_walk_still_reads_the_next_folder() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("watched");
    for name in ["Release A", "Release B"] {
        let folder = root.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("track.flac"), flac()).unwrap();
    }
    test.service
        .library_manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();
    let (reader, mut entered) = HeldListing::new();
    let reader = Arc::new(reader);
    let (scan, mut events) = test.scan_with(
        Arc::new(crate::import::file_tag_snapshot::LoftyFileTagReader),
        reader.clone(),
    );

    let observe = async {
        let _opens_on_failure = OpensOnDrop(&reader);
        let held = tokio::time::timeout(Duration::from_secs(5), entered.recv())
            .await
            .expect("the walk reaches a second folder")
            .expect("the reader is still held");
        let first = reader
            .first
            .lock()
            .unwrap()
            .clone()
            .expect("a folder was read before the held one");
        let announced = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let crate::import::handle::ImportEvent::Scan(
                    ScanEvent::FolderCandidate { candidate, .. }
                    | ScanEvent::CandidateDiscovered { candidate, .. },
                ) = events.recv().await.expect("the scan is still announcing")
                {
                    break candidate.path;
                }
            }
        })
        .await
        .expect("the folder read before the held one is announced while it is held");
        reader.open();
        (root.join(first), root.join(held), announced)
    };
    let (result, (first, held, announced)) = tokio::join!(scan.rescan(&root), observe);
    result.expect("the scan completes once the held folder opens");

    assert_ne!(first, held);
    assert_eq!(announced, first);
    assert!(
        announced_candidates(&mut events)
            .iter()
            .any(|path| root.join(path) == held),
        "the held folder is announced once its listing opens"
    );
}
