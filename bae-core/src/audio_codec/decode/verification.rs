use super::*;

/// The import verifier's sink decode. Invalid compressed packets are discarded
/// and reported through [`DecodedSink::add_discarded_packet_count`], so the sink
/// can decide whether decoded-frame coverage proves the track remains usable.
/// Whole-file collection and save keep rejecting the same packet; streaming
/// playback applies the same discard policy and reports the packet as an error.
pub(crate) fn decode_audio_to_verifying_sink(
    buffer: SharedSparseBuffer,
    start_sample: Option<u64>,
    end_sample: Option<u64>,
    sink: &mut dyn DecodedSink,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    decode_audio_to_sink_with_handling(
        buffer,
        None,
        start_sample,
        start_sample,
        end_sample,
        InvalidPacketHandling::Discard,
        sink,
        cancel,
    )
}
