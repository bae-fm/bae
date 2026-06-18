//! A focused parser for the ICU MessageFormat 1 subset bae authors.
//!
//! Supported: literal text, simple named arguments (`{name}`), and a `plural`
//! argument (`{name, plural, one {# item} other {# items}}`) whose cases are
//! CLDR categories (`zero`/`one`/`two`/`few`/`many`/`other`) or exact matches
//! (`=0`). `#` inside a plural case is the formatted count. Apostrophe quoting
//! follows ICU: `''` is a literal apostrophe, and `'{'` / `'}'` / `'#'` quote a
//! literal special character.
//!
//! Deliberately narrow: `select`/`selectordinal`, number/date skeletons, and
//! nested complex args are rejected with a clear error rather than mis-parsed,
//! so an unsupported construct breaks the build instead of emitting wrong
//! output. The set widens when a real message needs more.

/// One node of a parsed message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Text(String),
    /// A simple named argument: `{name}`.
    Arg(String),
    /// `#` — the formatted count, only meaningful inside a plural case.
    Pound,
    /// `{name, plural, <cases>}`.
    Plural {
        arg: String,
        cases: Vec<PluralCase>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluralCase {
    pub selector: PluralSelector,
    pub message: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluralSelector {
    Category(PluralCategory),
    /// `=N` exact match, checked before the category rules.
    Exact(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    pub fn as_cldr(self) -> &'static str {
        match self {
            PluralCategory::Zero => "zero",
            PluralCategory::One => "one",
            PluralCategory::Two => "two",
            PluralCategory::Few => "few",
            PluralCategory::Many => "many",
            PluralCategory::Other => "other",
        }
    }

    fn from_keyword(kw: &str) -> Option<Self> {
        Some(match kw {
            "zero" => PluralCategory::Zero,
            "one" => PluralCategory::One,
            "two" => PluralCategory::Two,
            "few" => PluralCategory::Few,
            "many" => PluralCategory::Many,
            "other" => PluralCategory::Other,
            _ => return None,
        })
    }
}

/// Parse an MF1 source string into nodes. Returns a human-readable error on any
/// unsupported or malformed construct.
pub fn parse(src: &str) -> Result<Vec<Node>, String> {
    let mut p = Parser {
        chars: src.chars().collect(),
        pos: 0,
    };
    let nodes = p.parse_message(true)?;
    if p.pos != p.chars.len() {
        return Err(format!("unexpected `}}` at offset {}", p.pos));
    }
    Ok(nodes)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }

    /// Parse a run of message nodes. `top` distinguishes the whole-message call
    /// (stops at end of input) from a plural case body (stops at the matching
    /// `}`, which it consumes).
    fn parse_message(&mut self, top: bool) -> Result<Vec<Node>, String> {
        let mut nodes = Vec::new();
        let mut text = String::new();
        loop {
            match self.peek() {
                None => {
                    if !top {
                        return Err("unterminated `{` — missing `}`".to_string());
                    }
                    break;
                }
                Some('}') => {
                    if top {
                        // Caller (parse) flags the stray `}`.
                        break;
                    }
                    self.bump(); // consume the closing brace of a case body
                    break;
                }
                Some('\'') => {
                    self.parse_quoted(&mut text)?;
                }
                Some('#') => {
                    self.bump();
                    flush(&mut nodes, &mut text);
                    nodes.push(Node::Pound);
                }
                Some('{') => {
                    flush(&mut nodes, &mut text);
                    nodes.push(self.parse_arg()?);
                }
                Some(c) => {
                    self.bump();
                    text.push(c);
                }
            }
        }
        flush(&mut nodes, &mut text);
        Ok(nodes)
    }

    /// At an apostrophe. ICU rules: `''` -> literal `'`; `'<special>'` quotes a
    /// literal run until the next lone apostrophe; a lone apostrophe before a
    /// non-special char is itself literal.
    fn parse_quoted(&mut self, text: &mut String) -> Result<(), String> {
        self.bump(); // opening '
        match self.peek() {
            Some('\'') => {
                self.bump();
                text.push('\'');
                Ok(())
            }
            Some(c) if c == '{' || c == '}' || c == '#' => {
                // Quoted literal run until the next apostrophe.
                while let Some(c) = self.peek() {
                    if c == '\'' {
                        self.bump();
                        return Ok(());
                    }
                    text.push(c);
                    self.bump();
                }
                Err("unterminated apostrophe quote".to_string())
            }
            _ => {
                // Lone apostrophe, literal.
                text.push('\'');
                Ok(())
            }
        }
    }

    /// At `{`. Parses either a simple `{name}` or `{name, plural, ...}`.
    fn parse_arg(&mut self) -> Result<Node, String> {
        self.bump(); // {
        self.skip_ws();
        let name = self.parse_ident()?;
        self.skip_ws();
        match self.bump() {
            Some('}') => Ok(Node::Arg(name)),
            Some(',') => {
                self.skip_ws();
                let kind = self.parse_ident()?;
                if kind != "plural" {
                    return Err(format!(
                        "argument `{name}` uses unsupported `{kind}` (only `plural` is supported)"
                    ));
                }
                self.skip_ws();
                if self.bump() != Some(',') {
                    return Err(format!("expected `,` after `plural` in `{name}`"));
                }
                let cases = self.parse_plural_cases()?;
                Ok(Node::Plural { arg: name, cases })
            }
            other => Err(format!(
                "expected `}}` or `,` after argument name `{name}`, found {other:?}"
            )),
        }
    }

    fn parse_plural_cases(&mut self) -> Result<Vec<PluralCase>, String> {
        let mut cases = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                Some('}') => {
                    self.bump();
                    break;
                }
                None => return Err("unterminated plural — missing `}`".to_string()),
                _ => {}
            }
            let selector = self.parse_selector()?;
            self.skip_ws();
            if self.bump() != Some('{') {
                return Err("expected `{` to open a plural case body".to_string());
            }
            let message = self.parse_message(false)?;
            cases.push(PluralCase { selector, message });
        }
        if cases.is_empty() {
            return Err("plural has no cases".to_string());
        }
        if !cases
            .iter()
            .any(|c| matches!(c.selector, PluralSelector::Category(PluralCategory::Other)))
        {
            return Err("plural must include an `other` case".to_string());
        }
        Ok(cases)
    }

    fn parse_selector(&mut self) -> Result<PluralSelector, String> {
        if self.peek() == Some('=') {
            self.bump();
            let mut digits = String::new();
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                digits.push(self.bump().unwrap());
            }
            if digits.is_empty() {
                return Err("expected a number after `=` in a plural selector".to_string());
            }
            let n = digits
                .parse::<i64>()
                .map_err(|e| format!("bad `=` selector {digits:?}: {e}"))?;
            return Ok(PluralSelector::Exact(n));
        }
        let kw = self.parse_ident()?;
        PluralCategory::from_keyword(&kw)
            .map(PluralSelector::Category)
            .ok_or_else(|| format!("unknown plural selector `{kw}`"))
    }

    fn parse_ident(&mut self) -> Result<String, String> {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' || c == '.' {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        if s.is_empty() {
            return Err(format!("expected an identifier at offset {}", self.pos));
        }
        Ok(s)
    }
}

/// The argument names a message references, in order of first appearance
/// (a plural's own argument included). The generator derives positional
/// specifiers from this order, and the validator checks it against the declared
/// `args`.
pub fn referenced_args(nodes: &[Node]) -> Vec<String> {
    let mut order = Vec::new();
    fn walk(nodes: &[Node], order: &mut Vec<String>) {
        for n in nodes {
            match n {
                Node::Arg(name) => {
                    if !order.contains(name) {
                        order.push(name.clone());
                    }
                }
                Node::Plural { arg, cases } => {
                    if !order.contains(arg) {
                        order.push(arg.clone());
                    }
                    for c in cases {
                        walk(&c.message, order);
                    }
                }
                Node::Text(_) | Node::Pound => {}
            }
        }
    }
    walk(nodes, &mut order);
    order
}

fn flush(nodes: &mut Vec<Node>, text: &mut String) {
    if !text.is_empty() {
        nodes.push(Node::Text(std::mem::take(text)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text() {
        assert_eq!(
            parse("that release couldn't be found").unwrap(),
            vec![Node::Text("that release couldn't be found".to_string())]
        );
    }

    #[test]
    fn simple_args() {
        assert_eq!(
            parse("Looking up barcode {position} of {total}").unwrap(),
            vec![
                Node::Text("Looking up barcode ".to_string()),
                Node::Arg("position".to_string()),
                Node::Text(" of ".to_string()),
                Node::Arg("total".to_string()),
            ]
        );
    }

    #[test]
    fn plural_with_pound() {
        let nodes =
            parse("{count, plural, one {# pending delete} other {# pending deletes}}").unwrap();
        let Node::Plural { arg, cases } = &nodes[0] else {
            panic!("expected a plural node, got {nodes:?}");
        };
        assert_eq!(arg, "count");
        assert_eq!(cases.len(), 2);
        assert_eq!(
            cases[0].selector,
            PluralSelector::Category(PluralCategory::One)
        );
        assert_eq!(
            cases[0].message,
            vec![Node::Pound, Node::Text(" pending delete".to_string())]
        );
    }

    #[test]
    fn plural_exact_and_category() {
        let nodes = parse("{n, plural, =0 {none} one {# thing} other {# things}}").unwrap();
        let Node::Plural { cases, .. } = &nodes[0] else {
            panic!("expected plural");
        };
        assert_eq!(cases[0].selector, PluralSelector::Exact(0));
        assert_eq!(cases.len(), 3);
    }

    #[test]
    fn rejects_select() {
        let err = parse("{g, select, male {he} other {they}}").unwrap_err();
        assert!(err.contains("only `plural`"), "got: {err}");
    }

    #[test]
    fn rejects_plural_without_other() {
        let err = parse("{n, plural, one {#}}").unwrap_err();
        assert!(err.contains("`other`"), "got: {err}");
    }

    #[test]
    fn apostrophe_quotes_a_literal_brace() {
        assert_eq!(
            parse("a '{' b").unwrap(),
            vec![Node::Text("a { b".to_string())]
        );
    }

    #[test]
    fn double_apostrophe_is_literal() {
        assert_eq!(
            parse("it''s here").unwrap(),
            vec![Node::Text("it's here".to_string())]
        );
    }

    #[test]
    fn rejects_stray_close_brace() {
        assert!(parse("oops}").is_err());
    }
}
