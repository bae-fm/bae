//! The tables a fragment's SQL names after `FROM` and `JOIN`, read from the
//! SQL as the fragment's author wrote it: the text of a `fragment!` or
//! `read!` call, with its `@path::Fragment` spreads still in place.

use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Word(String),
    Punct(char),
}

impl Token {
    fn is_word(&self, word: &str) -> bool {
        matches!(self, Token::Word(text) if text == word)
    }
}

/// The words and punctuation of `sql`, with its comments, string literals and
/// quoted names left out of the words: a quoted name is a word of its own.
fn tokens(sql: &str) -> Vec<Token> {
    let chars: Vec<char> = sql.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c.is_whitespace() {
            at += 1;
        } else if c == '-' && chars.get(at + 1) == Some(&'-') {
            while at < chars.len() && chars[at] != '\n' {
                at += 1;
            }
        } else if c == '/' && chars.get(at + 1) == Some(&'*') {
            at += 2;
            while at < chars.len() && !(chars[at] == '*' && chars.get(at + 1) == Some(&'/')) {
                at += 1;
            }
            at += 2;
        } else if c == '\'' {
            at = after_quoted(&chars, at, '\'').0;
        } else if c == '"' || c == '`' || c == '[' {
            let close = if c == '[' { ']' } else { c };
            let (end, text) = after_quoted(&chars, at, close);
            tokens.push(Token::Word(text.to_lowercase()));
            at = end;
        } else if c.is_alphabetic() || c == '_' {
            let start = at;
            while at < chars.len() && (chars[at].is_alphanumeric() || chars[at] == '_') {
                at += 1;
            }
            tokens.push(Token::Word(chars[start..at].iter().collect::<String>().to_lowercase()));
        } else {
            tokens.push(Token::Punct(c));
            at += 1;
        }
    }
    tokens
}

/// Where the text quoted with `close` ends, for a quote opened at `open`, and
/// the text. A doubled `close` inside it is one `close` of the text.
fn after_quoted(chars: &[char], open: usize, close: char) -> (usize, String) {
    let mut text = String::new();
    let mut at = open + 1;
    while at < chars.len() {
        if chars[at] == close {
            if close != ']' && chars.get(at + 1) == Some(&close) {
                text.push(close);
                at += 2;
                continue;
            }
            return (at + 1, text);
        }
        text.push(chars[at]);
        at += 1;
    }
    (at, text)
}

/// Words that end the list of `FROM` items at their nesting level.
const ENDS_FROM_ITEMS: [&str; 9] =
    ["where", "group", "order", "having", "limit", "union", "intersect", "except", "window"];

/// The names `sql` declares as common table expressions: `name AS (` and
/// `name (columns) AS (`.
fn declared_names(tokens: &[Token]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for (at, token) in tokens.iter().enumerate() {
        if !token.is_word("as") || tokens.get(at + 1) != Some(&Token::Punct('(')) {
            continue;
        }
        let before = at.checked_sub(1).map(|index| &tokens[index]);
        match before {
            Some(Token::Word(name)) => {
                names.insert(name.clone());
            }
            Some(Token::Punct(')')) => {
                let mut depth = 0;
                for index in (0..at).rev() {
                    match tokens[index] {
                        Token::Punct(')') => depth += 1,
                        Token::Punct('(') => {
                            depth -= 1;
                            if depth == 0 {
                                if let Some(Token::Word(name)) =
                                    index.checked_sub(1).map(|before| &tokens[before])
                                {
                                    names.insert(name.clone());
                                }
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    names
}

/// The tables `sql` reads by name, lowercase: each name after `FROM`, `JOIN`
/// or a comma between `FROM` items, which is no common table expression of
/// `sql`, no function and no spread. A schema's prefix (`main.`) is dropped.
pub(crate) fn tables_read(sql: &str) -> BTreeSet<String> {
    let tokens = tokens(sql);
    let declared = declared_names(&tokens);
    let mut tables = BTreeSet::new();
    // For each open parenthesis, whether its level is in a `FROM` list.
    let mut in_from = vec![false];
    let mut expecting = false;
    let mut at = 0;
    while at < tokens.len() {
        let token = &tokens[at];
        at += 1;
        match token {
            Token::Punct('(') => {
                in_from.push(false);
                expecting = false;
            }
            Token::Punct(')') => {
                if in_from.len() > 1 {
                    in_from.pop();
                }
                expecting = false;
            }
            Token::Punct(',') => {
                expecting = in_from.last().copied().unwrap_or(false);
            }
            Token::Punct(_) => expecting = false,
            Token::Word(word) if word == "from" || word == "join" => {
                if let Some(level) = in_from.last_mut() {
                    *level = true;
                }
                expecting = true;
            }
            Token::Word(word) if ENDS_FROM_ITEMS.contains(&word.as_str()) => {
                if let Some(level) = in_from.last_mut() {
                    *level = false;
                }
                expecting = false;
            }
            Token::Word(word) if expecting => {
                expecting = false;
                let mut name = word.clone();
                if tokens.get(at) == Some(&Token::Punct('.')) {
                    if let Some(Token::Word(table)) = tokens.get(at + 1) {
                        name = table.clone();
                        at += 2;
                    }
                }
                let is_function = tokens.get(at) == Some(&Token::Punct('('));
                if !is_function && !declared.contains(&name) {
                    tables.insert(name);
                }
            }
            Token::Word(_) => {}
        }
    }
    tables
}

#[cfg(test)]
#[path = "sql_tables_tests.rs"]
mod tests;
