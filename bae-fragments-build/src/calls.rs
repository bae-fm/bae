//! The `sqlive_compose::fragment!` and `sqlive_compose::read!` calls of a
//! source file, as the compiler finds them: each call's name, the
//! line it starts on and the SQL it holds.

use proc_macro2::{TokenStream, TokenTree};
use syn::visit::Visit;

/// One macro call of a source file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Call {
    /// The macro: `fragment` or `read`.
    pub(crate) macro_name: String,
    /// The name its header gives the fragment or read.
    pub(crate) name: String,
    /// The line the call starts on.
    pub(crate) line: usize,
    /// The SQL the call holds.
    pub(crate) sql: String,
}

struct Calls(Vec<Call>);

impl<'ast> Visit<'ast> for Calls {
    fn visit_macro(&mut self, call: &'ast syn::Macro) {
        let segments: Vec<String> =
            call.path.segments.iter().map(|segment| segment.ident.to_string()).collect();
        if let [crate_name, macro_name] = segments.as_slice() {
            if crate_name == "sqlive_compose" && (macro_name == "fragment" || macro_name == "read")
            {
                if let Some(found) = call_of(macro_name, call) {
                    self.0.push(found);
                }
            }
        }
        syn::visit::visit_macro(self, call);
    }
}

/// The call `call` makes of the macro `macro_name`, when it is written as the
/// compiler reads it: a name first and the SQL as a string last.
fn call_of(macro_name: &str, call: &syn::Macro) -> Option<Call> {
    let trees: Vec<TokenTree> = call.tokens.clone().into_iter().collect();
    let name = match trees.first()? {
        TokenTree::Ident(name) => name.to_string(),
        _ => return None,
    };
    let sql = match trees.last()? {
        TokenTree::Literal(literal) => {
            syn::parse2::<syn::LitStr>(TokenStream::from(TokenTree::Literal(literal.clone())))
                .ok()?
                .value()
        }
        _ => return None,
    };
    Some(Call {
        macro_name: macro_name.to_owned(),
        name,
        line: call.path.segments.first()?.ident.span().start().line,
        sql,
    })
}

/// The calls of the Rust source `text`, or why it doesn't parse.
pub(crate) fn calls_of(text: &str) -> Result<Vec<Call>, syn::Error> {
    let file = syn::parse_file(text)?;
    let mut calls = Calls(Vec::new());
    calls.visit_file(&file);
    Ok(calls.0)
}

#[cfg(test)]
#[path = "calls_tests.rs"]
mod tests;
