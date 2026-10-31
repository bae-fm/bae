//! One open of a library, as the app follows it: the stream its steps are
//! reported in, which the call that opens the library writes.

use std::sync::{Arc, Mutex};

use crate::keyed_values::{KeyedValueStream, StreamValues, StreamWriter};
use crate::types::{BridgeFastValue, BridgeOpenProgress, KeyActivityCallback};

/// The app makes one for each call that opens a library, starts reading its
/// steps with [`Self::feed_open_steps`], then hands it to that call.
#[derive(uniffi::Object)]
pub struct BridgeLibraryOpening {
    /// Where the open reports its steps: the stream
    /// [`Self::feed_open_steps`] last handed out.
    steps: Mutex<Option<Arc<StreamValues>>>,
}

#[uniffi::export]
impl BridgeLibraryOpening {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            steps: Mutex::new(None),
        })
    }

    /// The stream the open reports its steps in: its one key, `""`, while
    /// the open runs a step, and nothing once the open ends. A cancelled
    /// stream holds nothing from then on.
    pub fn feed_open_steps(&self, activity: Box<dyn KeyActivityCallback>) -> Arc<KeyedValueStream> {
        let values = StreamValues::new(activity);
        *self.steps.lock().unwrap() = Some(values.clone());
        Arc::new(KeyedValueStream::new(
            values,
            bae_core::library::CancellationToken::new(),
        ))
    }
}

impl BridgeLibraryOpening {
    /// Write each step the open reports into the stream; dropping what this
    /// returns, as the open ends, empties the stream. An opening nobody reads
    /// writes nothing.
    pub(crate) fn report(&self) -> impl Fn(coven::Progress<coven::OpenStep>) + Send + 'static {
        let writer = self.steps.lock().unwrap().clone().map(StreamWriter::new);
        move |progress| {
            if let Some(writer) = &writer {
                writer.set(
                    String::new(),
                    BridgeFastValue::OpenProgress {
                        progress: BridgeOpenProgress::from_core(progress),
                    },
                );
            }
        }
    }
}
