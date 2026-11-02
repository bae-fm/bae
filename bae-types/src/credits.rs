//! A credit list as it prints: every album's and track's artist line is built
//! here, from each credit's printed name and what its source prints after it.

/// What is printed after a credit when the source states nothing: the
/// separator of a list with no joining words.
pub const DEFAULT_JOIN: &str = ", ";

/// The line a credit list prints: each credit's printed name, then its join
/// ([`DEFAULT_JOIN`] where it states none). The last credit's join is never
/// printed: nothing follows it.
pub fn credit_line<'a>(credits: impl IntoIterator<Item = (&'a str, Option<&'a str>)>) -> String {
    let mut line = String::new();
    let mut pending_join: Option<&str> = None;
    for (name, join) in credits {
        if let Some(previous) = pending_join {
            line.push_str(previous);
        }
        line.push_str(name);
        pending_join = Some(join.unwrap_or(DEFAULT_JOIN));
    }
    line
}

/// A join as bae keeps it: `None` for nothing stated, so the default prints.
pub fn stated_join(join: Option<&str>) -> Option<String> {
    join.filter(|join| !join.is_empty()).map(str::to_string)
}

/// The joins of a credit list as stored: the last credit's join is dropped,
/// since nothing follows it, so removing the credit after another leaves that
/// one printing no join.
pub fn drop_last_join<T>(credits: &mut [T], join: impl Fn(&mut T) -> &mut Option<String>) {
    if let Some(last) = credits.last_mut() {
        *join(last) = None;
    }
}

#[cfg(test)]
#[path = "credits_tests.rs"]
mod tests;
