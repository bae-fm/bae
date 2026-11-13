//! A list of MusicBrainz attribute values read at compile time.
//!
//! Each list is a captured text of lines holding an id, a tab and a name, and
//! lines beginning with `#` for the capture's header. Reading it in a constant
//! leaves nothing to parse when a credit is read or worded.

/// The id and name of each entry of `list`, in its order.
pub(super) type Entries<const N: usize> = [(&'static str, &'static str); N];

const fn is_entry(line: &[u8]) -> bool {
    !line.is_empty() && line[0] != b'#'
}

/// The number of entries in `list`.
pub(super) const fn count(list: &str) -> usize {
    let bytes = list.as_bytes();
    let mut entries = 0;
    let mut start = 0;
    let mut at = 0;
    while at <= bytes.len() {
        if at == bytes.len() || bytes[at] == b'\n' {
            let (_, rest) = bytes.split_at(start);
            let (line, _) = rest.split_at(at - start);
            if is_entry(line) {
                entries += 1;
            }
            start = at + 1;
        }
        at += 1;
    }
    entries
}

const fn text(bytes: &'static [u8]) -> &'static str {
    match core::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => panic!("a list is UTF-8"),
    }
}

/// The entries of `list`, which holds exactly `N` of them.
pub(super) const fn parse<const N: usize>(list: &'static str) -> Entries<N> {
    let bytes = list.as_bytes();
    let mut entries: Entries<N> = [("", ""); N];
    let mut filled = 0;
    let mut start = 0;
    let mut at = 0;
    while at <= bytes.len() {
        if at == bytes.len() || bytes[at] == b'\n' {
            let (_, rest) = bytes.split_at(start);
            let (line, _) = rest.split_at(at - start);
            if is_entry(line) {
                let mut tab = 0;
                while tab < line.len() && line[tab] != b'\t' {
                    tab += 1;
                }
                if tab == line.len() {
                    panic!("a list line holds an id and a name");
                }
                let (id, rest) = line.split_at(tab);
                let (_, name) = rest.split_at(1);
                entries[filled] = (text(id), text(name));
                filled += 1;
            }
            start = at + 1;
        }
        at += 1;
    }
    entries
}
