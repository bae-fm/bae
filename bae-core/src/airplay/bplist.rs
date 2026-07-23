//! A minimal Apple binary property list (`bplist00`) codec.
//!
//! The AirPlay 2 control messages — SETUP, SETPEERS, SETRATEANCHORTIME — carry
//! their bodies as binary plists (`application/x-apple-binary-plist`). This
//! encodes the value tree those bodies need (dicts, arrays, strings, integers,
//! reals, booleans, byte blobs) into the CoreFoundation format, and decodes a
//! receiver's plist responses back. Only the object types the AirPlay wire uses
//! are handled; the format is otherwise the documented `bplist00` layout: an
//! 8-byte header, the packed objects, an offset table, and a 32-byte trailer.

use std::collections::BTreeMap;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BplistError {
    /// The `bplist00` header was missing.
    BadHeader,
    /// The message ended before a complete object was read.
    Truncated,
    /// An object marker named a type this codec doesn't handle.
    UnsupportedType(u8),
}

impl std::fmt::Display for BplistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BplistError::BadHeader => write!(f, "not a bplist00 payload"),
            BplistError::Truncated => write!(f, "truncated bplist"),
            BplistError::UnsupportedType(m) => write!(f, "unsupported bplist marker {m:#04x}"),
        }
    }
}

impl std::error::Error for BplistError {}

/// Decode a `bplist00` payload to a value tree.
pub fn decode(bytes: &[u8]) -> Result<Plist, BplistError> {
    if bytes.len() < 8 + 32 || &bytes[..8] != b"bplist00" {
        return Err(BplistError::BadHeader);
    }
    let trailer = &bytes[bytes.len() - 32..];
    let offset_size = trailer[6] as usize;
    let ref_size = trailer[7] as usize;
    let num_objects = u64::from_be_bytes(trailer[8..16].try_into().unwrap()) as usize;
    let top = u64::from_be_bytes(trailer[16..24].try_into().unwrap()) as usize;
    let table_offset = u64::from_be_bytes(trailer[24..32].try_into().unwrap()) as usize;

    // The offset table: `num_objects` entries of `offset_size` bytes each.
    let mut offsets = Vec::with_capacity(num_objects);
    for i in 0..num_objects {
        let start = table_offset + i * offset_size;
        let slice = bytes
            .get(start..start + offset_size)
            .ok_or(BplistError::Truncated)?;
        offsets.push(read_int_be(slice));
    }

    let ctx = DecodeCtx {
        bytes,
        offsets: &offsets,
        ref_size,
    };
    ctx.object(top)
}

struct DecodeCtx<'a> {
    bytes: &'a [u8],
    offsets: &'a [usize],
    ref_size: usize,
}

impl DecodeCtx<'_> {
    fn object(&self, index: usize) -> Result<Plist, BplistError> {
        let at = *self.offsets.get(index).ok_or(BplistError::Truncated)?;
        let marker = *self.bytes.get(at).ok_or(BplistError::Truncated)?;
        let ty = marker >> 4;
        let low = (marker & 0x0F) as usize;
        match ty {
            0x0 => match marker {
                0x08 => Ok(Plist::Bool(false)),
                0x09 => Ok(Plist::Bool(true)),
                _ => Err(BplistError::UnsupportedType(marker)),
            },
            0x1 => {
                let width = 1usize << low;
                let slice = self
                    .bytes
                    .get(at + 1..at + 1 + width)
                    .ok_or(BplistError::Truncated)?;
                Ok(Plist::Integer(read_int_be(slice) as u64))
            }
            0x2 => {
                let width = 1usize << low;
                let slice = self
                    .bytes
                    .get(at + 1..at + 1 + width)
                    .ok_or(BplistError::Truncated)?;
                let real = if width == 4 {
                    f64::from(f32::from_be_bytes(slice.try_into().unwrap()))
                } else {
                    f64::from_be_bytes(slice.try_into().unwrap())
                };
                Ok(Plist::Real(real))
            }
            0x4 => {
                let (count, data_at) = self.count(at, low)?;
                let slice = self
                    .bytes
                    .get(data_at..data_at + count)
                    .ok_or(BplistError::Truncated)?;
                Ok(Plist::Data(slice.to_vec()))
            }
            0x5 => {
                let (count, data_at) = self.count(at, low)?;
                let slice = self
                    .bytes
                    .get(data_at..data_at + count)
                    .ok_or(BplistError::Truncated)?;
                Ok(Plist::String(String::from_utf8_lossy(slice).into_owned()))
            }
            0x6 => {
                let (count, data_at) = self.count(at, low)?;
                let mut units = Vec::with_capacity(count);
                for i in 0..count {
                    let s = data_at + i * 2;
                    let pair = self.bytes.get(s..s + 2).ok_or(BplistError::Truncated)?;
                    units.push(u16::from_be_bytes(pair.try_into().unwrap()));
                }
                Ok(Plist::String(String::from_utf16_lossy(&units)))
            }
            0xA => {
                let (count, refs_at) = self.count(at, low)?;
                let mut items = Vec::with_capacity(count);
                for i in 0..count {
                    items.push(self.object(self.reference(refs_at, i)?)?);
                }
                Ok(Plist::Array(items))
            }
            0xD => {
                let (count, refs_at) = self.count(at, low)?;
                let mut entries = Vec::with_capacity(count);
                // Preserve wire order but expose deterministically: keys then vals.
                let mut keyed = BTreeMap::new();
                for i in 0..count {
                    let key = self.object(self.reference(refs_at, i)?)?;
                    let val = self.object(self.reference(refs_at, count + i)?)?;
                    if let Plist::String(k) = key {
                        keyed.insert(i, (k, val));
                    }
                }
                for (_, (k, v)) in keyed {
                    entries.push((k, v));
                }
                Ok(Plist::Dict(entries))
            }
            _ => Err(BplistError::UnsupportedType(marker)),
        }
    }

    /// The element count for a collection/string/data marker and the offset just
    /// past the count (where the payload or refs begin).
    fn count(&self, at: usize, low: usize) -> Result<(usize, usize), BplistError> {
        if low != 0x0F {
            return Ok((low, at + 1));
        }
        // An inline integer object holds the real count.
        let int_marker = *self.bytes.get(at + 1).ok_or(BplistError::Truncated)?;
        let width = 1usize << (int_marker & 0x0F);
        let slice = self
            .bytes
            .get(at + 2..at + 2 + width)
            .ok_or(BplistError::Truncated)?;
        Ok((read_int_be(slice), at + 2 + width))
    }

    fn reference(&self, refs_at: usize, i: usize) -> Result<usize, BplistError> {
        let start = refs_at + i * self.ref_size;
        let slice = self
            .bytes
            .get(start..start + self.ref_size)
            .ok_or(BplistError::Truncated)?;
        Ok(read_int_be(slice))
    }
}

fn read_int_be(bytes: &[u8]) -> usize {
    let mut v = 0usize;
    for &b in bytes {
        v = (v << 8) | b as usize;
    }
    v
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
}
