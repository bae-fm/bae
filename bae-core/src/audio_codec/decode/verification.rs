use super::*;

/// The import verifier's sink decode. Invalid compressed packets are discarded
/// and reported through [`DecodedSink::set_discarded_packet_count`], so the sink
/// can decide whether decoded-frame coverage proves the track remains usable.
/// Whole-file collection, playback, and save keep rejecting the same packet.
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
