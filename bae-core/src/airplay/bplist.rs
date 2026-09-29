//! A minimal Apple binary property list (`bplist00`) codec.
//!
//! The AirPlay 2 control messages — SETUP, SETPEERS, SETRATEANCHORTIME — carry
//! their bodies as binary plists (`application/x-apple-binary-plist`). This
//! encodes the value tree those bodies need (dicts, arrays, strings, integers,
//! reals, booleans, byte blobs) into the CoreFoundation format, and decodes a
//! receiver's plist responses back. Only the object types the AirPlay wire uses
//! are handled; the format is otherwise the documented `bplist00` layout: an
//! 8-byte header, the packed objects, an offset table, and a 32-byte trailer.

/// A property-list value.
#[derive(Debug, Clone, PartialEq)]
pub enum Plist {
    Bool(bool),
    /// A non-negative integer (AirPlay bodies use unsigned counts, ports, clocks).
    Integer(u64),
    Real(f64),
    String(String),
    /// A byte blob (e.g. the 32-byte audio session key `shk`).
    Data(Vec<u8>),
    Array(Vec<Plist>),
    /// An ordered dictionary — key order is preserved on encode.
    Dict(Vec<(String, Plist)>),
}

impl Plist {
    /// Look up a key in a dict value.
    pub fn get(&self, key: &str) -> Option<&Plist> {
        match self {
            Plist::Dict(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The integer value, if this is one.
    pub fn as_integer(&self) -> Option<u64> {
        match self {
            Plist::Integer(v) => Some(*v),
            _ => None,
        }
    }

    /// The string value, if this is one.
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Plist::String(s) => Some(s),
            _ => None,
        }
    }
}

/// A flattened object with its assigned index, ready to encode.
enum Node {
    Bool(bool),
    Integer(u64),
    Real(f64),
    String(String),
    Data(Vec<u8>),
    Array(Vec<usize>),
    Dict(Vec<usize>, Vec<usize>),
}

/// Encode a value tree to `bplist00` bytes.
pub fn encode(root: &Plist) -> Vec<u8> {
    let mut nodes: Vec<Node> = Vec::new();
    flatten(root, &mut nodes);
    let object_count = nodes.len();
    let ref_size = byte_width(object_count.saturating_sub(1) as u64);

    // Encode each object; the offset of object `i` is where its bytes start.
    let mut body = Vec::new();
    let mut offsets = Vec::with_capacity(object_count);
    const HEADER: &[u8] = b"bplist00";
    for node in &nodes {
        offsets.push(HEADER.len() + body.len());
        encode_node(node, ref_size, &mut body);
    }

    let offset_table_start = HEADER.len() + body.len();
    let offset_size = byte_width(offset_table_start as u64);

    let mut out = Vec::with_capacity(offset_table_start + object_count * offset_size + 32);
    out.extend_from_slice(HEADER);
    out.extend_from_slice(&body);
    for &offset in &offsets {
        out.extend_from_slice(&int_bytes(offset as u64, offset_size));
    }
    // Trailer: 5 unused + sort_version + offset_size + ref_size + num_objects(8)
    // + top_object(8) + offset_table_offset(8), all big-endian.
    out.extend_from_slice(&[0u8; 6]);
    out.push(offset_size as u8);
    out.push(ref_size as u8);
    out.extend_from_slice(&(object_count as u64).to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes()); // top object is index 0
    out.extend_from_slice(&(offset_table_start as u64).to_be_bytes());
    out
}

/// Assign `value` (and its descendants) object indices in `nodes`, returning
/// `value`'s index. A container is indexed before its children.
fn flatten(value: &Plist, nodes: &mut Vec<Node>) -> usize {
    let index = nodes.len();
    // Reserve this slot; containers fill it after their children are indexed.
    nodes.push(Node::Bool(false));
    let node = match value {
        Plist::Bool(b) => Node::Bool(*b),
        Plist::Integer(v) => Node::Integer(*v),
        Plist::Real(v) => Node::Real(*v),
        Plist::String(s) => Node::String(s.clone()),
        Plist::Data(d) => Node::Data(d.clone()),
        Plist::Array(items) => {
            let refs = items.iter().map(|item| flatten(item, nodes)).collect();
            Node::Array(refs)
        }
        Plist::Dict(entries) => {
            let key_refs = entries
                .iter()
                .map(|(k, _)| flatten(&Plist::String(k.clone()), nodes))
                .collect();
            let val_refs = entries.iter().map(|(_, v)| flatten(v, nodes)).collect();
            Node::Dict(key_refs, val_refs)
        }
    };
    nodes[index] = node;
    index
}

fn encode_node(node: &Node, ref_size: usize, out: &mut Vec<u8>) {
    match node {
        Node::Bool(false) => out.push(0x08),
        Node::Bool(true) => out.push(0x09),
        Node::Integer(v) => encode_integer(*v, out),
        Node::Real(v) => {
            out.push(0x23); // real, 8 bytes
            out.extend_from_slice(&v.to_be_bytes());
        }
        Node::String(s) => {
            // ASCII strings ride as-is; anything non-ASCII goes UTF-16BE.
            if s.is_ascii() {
                encode_marker(0x5, s.len(), out);
                out.extend_from_slice(s.as_bytes());
            } else {
                let units: Vec<u16> = s.encode_utf16().collect();
                encode_marker(0x6, units.len(), out);
                for u in units {
                    out.extend_from_slice(&u.to_be_bytes());
                }
            }
        }
        Node::Data(d) => {
            encode_marker(0x4, d.len(), out);
            out.extend_from_slice(d);
        }
        Node::Array(refs) => {
            encode_marker(0xA, refs.len(), out);
            for &r in refs {
                out.extend_from_slice(&int_bytes(r as u64, ref_size));
            }
        }
        Node::Dict(keys, vals) => {
            encode_marker(0xD, keys.len(), out);
            for &k in keys {
                out.extend_from_slice(&int_bytes(k as u64, ref_size));
            }
            for &v in vals {
                out.extend_from_slice(&int_bytes(v as u64, ref_size));
            }
        }
    }
}

/// Write a type marker (`type << 4`) with an inline count: the low nibble when it
/// fits (< 15), else `0xF` followed by an inline integer object.
fn encode_marker(ty: u8, count: usize, out: &mut Vec<u8>) {
    if count < 15 {
        out.push((ty << 4) | count as u8);
    } else {
        out.push((ty << 4) | 0x0F);
        encode_integer(count as u64, out);
    }
}

/// Encode an integer object: marker `0x1n` where `2^n` is the byte width, then
/// the big-endian bytes.
fn encode_integer(v: u64, out: &mut Vec<u8>) {
    let width = byte_width(v);
    let n = width.trailing_zeros() as u8; // 1→0, 2→1, 4→2, 8→3
    out.push(0x10 | n);
    out.extend_from_slice(&int_bytes(v, width));
}

/// The smallest power-of-two byte width (1, 2, 4, or 8) that holds `v`.
fn byte_width(v: u64) -> usize {
    if v <= 0xFF {
        1
    } else if v <= 0xFFFF {
        2
    } else if v <= 0xFFFF_FFFF {
        4
    } else {
        8
    }
}

/// `v` as `width` big-endian bytes.
fn int_bytes(v: u64, width: usize) -> Vec<u8> {
    v.to_be_bytes()[8 - width..].to_vec()
}

/// A malformed binary plist.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BplistError {
    /// The `bplist00` header was missing.
    #[error("not a bplist00 payload")]
    BadHeader,
    /// The message ended before a complete object was read.
    #[error("truncated bplist")]
    Truncated,
    /// An object marker named a type this codec doesn't handle.
    #[error("unsupported bplist marker {0:#04x}")]
    UnsupportedType(u8),
    /// The trailer's widths, object count, or top object disagree with the
    /// payload they describe.
    #[error("bplist trailer does not describe its payload")]
    BadTrailer,
    /// An integer or real marker named a byte width this codec doesn't read.
    #[error("bplist marker {0:#04x} has an unsupported width")]
    BadWidth(u8),
    /// A dictionary key was not a string.
    #[error("bplist dictionary key is not a string")]
    NonStringKey,
    /// Objects nest deeper than any AirPlay body does, which includes a
    /// collection that contains itself.
    #[error("bplist objects nest too deep")]
    TooDeep,
    /// Shared references name more objects than any AirPlay body holds.
    #[error("bplist references too many objects")]
    TooManyObjects,
}

/// How deep objects may nest. AirPlay bodies nest a few levels; anything
/// deeper is malformed, and a collection that contains itself reaches it too.
const MAX_DEPTH: usize = 32;

/// How many objects one decode may produce. A payload's objects are few, but
/// shared references can name one object many times; this bounds the tree that
/// sharing can multiply out to.
const MAX_DECODED_OBJECTS: usize = 1 << 16;

/// Decode a `bplist00` payload to a value tree. Every count, width, and
/// reference is checked against the payload before it is used, so a malformed
/// receiver response is an error, never an oversized allocation or a runaway
/// recursion.
pub fn decode(bytes: &[u8]) -> Result<Plist, BplistError> {
    if bytes.len() < 8 + 32 || &bytes[..8] != b"bplist00" {
        return Err(BplistError::BadHeader);
    }
    let body_end = bytes.len() - 32;
    let trailer = &bytes[body_end..];
    let offset_size = usize::from(trailer[6]);
    let ref_size = usize::from(trailer[7]);
    if !(1..=8).contains(&offset_size) || !(1..=8).contains(&ref_size) {
        return Err(BplistError::BadTrailer);
    }
    let num_objects = be_u64(&trailer[8..16]);
    let top = be_u64(&trailer[16..24]);
    let table_offset = be_u64(&trailer[24..32]);
    if top >= num_objects {
        return Err(BplistError::BadTrailer);
    }

    // The offset table: `num_objects` entries of `offset_size` bytes each,
    // all before the trailer.
    let table_end = num_objects
        .checked_mul(offset_size as u64)
        .and_then(|len| len.checked_add(table_offset))
        .filter(|end| *end <= body_end as u64)
        .ok_or(BplistError::BadTrailer)?;
    let table = &bytes[table_offset as usize..table_end as usize];
    let offsets = table
        .chunks_exact(offset_size)
        .map(read_uint)
        .collect::<Result<Vec<_>, _>>()?;

    let ctx = DecodeCtx {
        bytes,
        offsets: &offsets,
        ref_size,
        decoded: std::cell::Cell::new(0),
    };
    ctx.object(top as usize, 0)
}

/// A big-endian unsigned integer of at most eight bytes.
fn be_u64(field: &[u8]) -> u64 {
    field
        .iter()
        .fold(0, |value, &byte| (value << 8) | u64::from(byte))
}

struct DecodeCtx<'a> {
    bytes: &'a [u8],
    offsets: &'a [usize],
    ref_size: usize,
    /// Objects produced so far, against [`MAX_DECODED_OBJECTS`].
    decoded: std::cell::Cell<usize>,
}

impl DecodeCtx<'_> {
    fn object(&self, index: usize, depth: usize) -> Result<Plist, BplistError> {
        if depth > MAX_DEPTH {
            return Err(BplistError::TooDeep);
        }
        let decoded = self.decoded.get() + 1;
        if decoded > MAX_DECODED_OBJECTS {
            return Err(BplistError::TooManyObjects);
        }
        self.decoded.set(decoded);

        let at = *self.offsets.get(index).ok_or(BplistError::Truncated)?;
        let marker = *self.bytes.get(at).ok_or(BplistError::Truncated)?;
        let ty = marker >> 4;
        let low = marker & 0x0F;
        match ty {
            0x0 => match marker {
                0x08 => Ok(Plist::Bool(false)),
                0x09 => Ok(Plist::Bool(true)),
                _ => Err(BplistError::UnsupportedType(marker)),
            },
            0x1 => {
                let width = integer_width(marker)?;
                Ok(Plist::Integer(be_u64(self.span(at + 1, width)?)))
            }
            0x2 => {
                let real = match low {
                    2 => f64::from(f32::from_bits(be_u64(self.span(at + 1, 4)?) as u32)),
                    3 => f64::from_bits(be_u64(self.span(at + 1, 8)?)),
                    _ => return Err(BplistError::BadWidth(marker)),
                };
                Ok(Plist::Real(real))
            }
            0x4 => {
                let (count, data_at) = self.count(at, low)?;
                Ok(Plist::Data(self.span(data_at, count)?.to_vec()))
            }
            0x5 => {
                let (count, data_at) = self.count(at, low)?;
                let slice = self.span(data_at, count)?;
                Ok(Plist::String(String::from_utf8_lossy(slice).into_owned()))
            }
            0x6 => {
                let (count, data_at) = self.count(at, low)?;
                let byte_len = count.checked_mul(2).ok_or(BplistError::Truncated)?;
                let units: Vec<u16> = self
                    .span(data_at, byte_len)?
                    .chunks_exact(2)
                    .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                    .collect();
                Ok(Plist::String(String::from_utf16_lossy(&units)))
            }
            0xA => {
                let (count, refs_at) = self.count(at, low)?;
                let items = self
                    .references(refs_at, count)?
                    .map(|index| self.object(index?, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Plist::Array(items))
            }
            0xD => {
                let (count, refs_at) = self.count(at, low)?;
                let refs_len = count.checked_mul(2).ok_or(BplistError::Truncated)?;
                let refs = self
                    .references(refs_at, refs_len)?
                    .collect::<Result<Vec<_>, _>>()?;
                let (keys, values) = refs.split_at(count);
                keys.iter()
                    .zip(values)
                    .map(|(&key, &value)| {
                        let Plist::String(key) = self.object(key, depth + 1)? else {
                            return Err(BplistError::NonStringKey);
                        };
                        Ok((key, self.object(value, depth + 1)?))
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map(Plist::Dict)
            }
            _ => Err(BplistError::UnsupportedType(marker)),
        }
    }

    /// `len` bytes starting at `start`, or `Truncated` when the payload ends
    /// first.
    fn span(&self, start: usize, len: usize) -> Result<&[u8], BplistError> {
        start
            .checked_add(len)
            .and_then(|end| self.bytes.get(start..end))
            .ok_or(BplistError::Truncated)
    }

    /// The element count for a collection/string/data marker and the offset just
    /// past the count (where the payload or refs begin).
    fn count(&self, at: usize, low: u8) -> Result<(usize, usize), BplistError> {
        if low != 0x0F {
            return Ok((usize::from(low), at + 1));
        }
        // An inline integer object holds the real count.
        let int_marker = *self.bytes.get(at + 1).ok_or(BplistError::Truncated)?;
        if int_marker >> 4 != 0x1 {
            return Err(BplistError::UnsupportedType(int_marker));
        }
        let width = integer_width(int_marker)?;
        Ok((read_uint(self.span(at + 2, width)?)?, at + 2 + width))
    }

    /// The `count` object references starting at `refs_at`, each checked to lie
    /// inside the payload before any is followed.
    fn references(
        &self,
        refs_at: usize,
        count: usize,
    ) -> Result<impl Iterator<Item = Result<usize, BplistError>> + '_, BplistError> {
        let len = count
            .checked_mul(self.ref_size)
            .ok_or(BplistError::Truncated)?;
        Ok(self
            .span(refs_at, len)?
            .chunks_exact(self.ref_size)
            .map(read_uint))
    }
}

/// The byte width an integer marker names: 1, 2, 4, or 8. The format's 16-byte
/// integers carry negative values, which no AirPlay body uses.
fn integer_width(marker: u8) -> Result<usize, BplistError> {
    match marker & 0x0F {
        low @ 0..=3 => Ok(1 << low),
        _ => Err(BplistError::BadWidth(marker)),
    }
}

/// A big-endian unsigned integer of at most eight bytes, as an index or length.
fn read_uint(bytes: &[u8]) -> Result<usize, BplistError> {
    let value = be_u64(bytes);
    usize::try_from(value).map_err(|_| BplistError::Truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(entries: &[(&str, Plist)]) -> Plist {
        Plist::Dict(
            entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    /// A small dict encodes to a well-formed bplist and decodes back unchanged.
    #[test]
    fn round_trips_a_flat_dict() {
        let value = dict(&[("rate", Plist::Integer(1)), ("rtpTime", Plist::Integer(0))]);
        let bytes = encode(&value);
        assert_eq!(&bytes[..8], b"bplist00");
        assert_eq!(decode(&bytes).unwrap(), value);
    }

    /// Nested arrays, data blobs, big integers, and long strings survive a round
    /// trip — the shapes the SETUP/SETPEERS bodies use.
    #[test]
    fn round_trips_nested_structures() {
        let value = dict(&[
            (
                "streams",
                Plist::Array(vec![dict(&[
                    ("type", Plist::Integer(96)),
                    ("shk", Plist::Data(vec![0xAB; 32])),
                    ("latencyMax", Plist::Integer(88_200)),
                    ("timingProtocol", Plist::String("NTP".to_string())),
                ])]),
            ),
            (
                "sessionUUID",
                Plist::String("3195C737-1E6E-4487-BECB-4D287B7C7626".to_string()),
            ),
            (
                "networkTimeTimelineID",
                Plist::Integer(0x1122_3344_5566_7788),
            ),
        ]);
        let bytes = encode(&value);
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded, value);
        // Reach into the decoded tree the way the response parser does.
        let shk = decoded
            .get("streams")
            .and_then(|s| match s {
                Plist::Array(items) => items.first(),
                _ => None,
            })
            .and_then(|s| s.get("shk"))
            .unwrap();
        assert_eq!(shk, &Plist::Data(vec![0xAB; 32]));
    }

    /// An array of IP strings — the SETPEERS body — round-trips.
    #[test]
    fn round_trips_string_array() {
        let value = Plist::Array(vec![
            Plist::String("10.0.0.2".to_string()),
            Plist::String("10.0.0.9".to_string()),
        ]);
        assert_eq!(decode(&encode(&value)).unwrap(), value);
    }

    /// The trailer records the object count and a top index of 0.
    #[test]
    fn trailer_is_well_formed() {
        let bytes = encode(&dict(&[("rate", Plist::Integer(0))]));
        let trailer = &bytes[bytes.len() - 32..];
        // dict + key string "rate" + value int 0 = 3 objects.
        assert_eq!(u64::from_be_bytes(trailer[8..16].try_into().unwrap()), 3);
        assert_eq!(u64::from_be_bytes(trailer[16..24].try_into().unwrap()), 0);
    }

    /// A booleans-and-reals value decodes to what was encoded.
    #[test]
    fn round_trips_bool_and_real() {
        let value = dict(&[
            ("on", Plist::Bool(true)),
            ("off", Plist::Bool(false)),
            ("gain", Plist::Real(0.5)),
        ]);
        assert_eq!(decode(&encode(&value)).unwrap(), value);
    }

    #[test]
    fn rejects_non_bplist() {
        assert_eq!(decode(b"not a plist").unwrap_err(), BplistError::BadHeader);
    }

    /// A payload laid out by hand: `objects` packed after the header, a
    /// one-byte offset table, and a trailer with one-byte refs. `num_objects`
    /// is what the trailer claims, which a malformed payload need not match.
    fn assemble(objects: &[&[u8]], num_objects: u64) -> Vec<u8> {
        let mut out = b"bplist00".to_vec();
        let mut offsets = Vec::new();
        for object in objects {
            offsets.push(u8::try_from(out.len()).expect("test payload fits one-byte offsets"));
            out.extend_from_slice(object);
        }
        let table_offset = out.len() as u64;
        out.extend_from_slice(&offsets);
        out.extend_from_slice(&[0; 6]);
        out.push(1);
        out.push(1);
        out.extend_from_slice(&num_objects.to_be_bytes());
        out.extend_from_slice(&0u64.to_be_bytes());
        out.extend_from_slice(&table_offset.to_be_bytes());
        out
    }

    #[test]
    fn rejects_an_object_count_the_payload_cannot_hold() {
        let bytes = assemble(&[&[0x09]], u64::MAX);
        assert_eq!(decode(&bytes).unwrap_err(), BplistError::BadTrailer);
    }

    #[test]
    fn rejects_a_real_of_a_width_that_is_not_four_or_eight_bytes() {
        let bytes = assemble(&[&[0x21, 0x00, 0x00]], 1);
        assert_eq!(decode(&bytes).unwrap_err(), BplistError::BadWidth(0x21));
    }

    #[test]
    fn rejects_an_integer_wider_than_eight_bytes() {
        let bytes = assemble(&[&[0x1F]], 1);
        assert_eq!(decode(&bytes).unwrap_err(), BplistError::BadWidth(0x1F));
    }

    #[test]
    fn rejects_a_collection_count_the_payload_cannot_hold() {
        // An array whose inline count is 2^64 - 1 refs.
        let array = [0xAF, 0x13, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let bytes = assemble(&[&array], 1);
        assert_eq!(decode(&bytes).unwrap_err(), BplistError::Truncated);
    }

    #[test]
    fn rejects_a_utf16_string_longer_than_the_payload() {
        let string = [0x6F, 0x13, 0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let bytes = assemble(&[&string], 1);
        assert_eq!(decode(&bytes).unwrap_err(), BplistError::Truncated);
    }

    #[test]
    fn rejects_an_array_that_contains_itself() {
        let bytes = assemble(&[&[0xA1, 0x00]], 1);
        assert_eq!(decode(&bytes).unwrap_err(), BplistError::TooDeep);
    }

    /// Twenty arrays, each holding the next one twice, name 2^20 objects
    /// through shared references while the payload holds twenty-one.
    #[test]
    fn rejects_shared_references_that_multiply_into_too_many_objects() {
        let arrays: Vec<[u8; 3]> = (1..=20u8).map(|next| [0xA2, next, next]).collect();
        let mut objects: Vec<&[u8]> = arrays.iter().map(|array| array.as_slice()).collect();
        objects.push(&[0x09]);
        let bytes = assemble(&objects, objects.len() as u64);
        assert_eq!(decode(&bytes).unwrap_err(), BplistError::TooManyObjects);
    }

    #[test]
    fn rejects_a_dict_key_that_is_not_a_string() {
        // {1: true}
        let bytes = assemble(&[&[0xD1, 0x01, 0x02], &[0x10, 0x01], &[0x09]], 3);
        assert_eq!(decode(&bytes).unwrap_err(), BplistError::NonStringKey);
    }
}
