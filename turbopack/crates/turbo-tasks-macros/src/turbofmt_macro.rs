use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Expr, ExprLit, Lit, Token, punctuated::Punctuated};

/// Parts of a format string, yielded by [`FormatIter`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FormatPart<'a> {
    /// Raw text between format specifiers.
    RawString(&'a str),
    /// An escaped brace: `{{` or `}}`.
    EscapedBrace(&'a str),
    /// A variable reference without format spec: `{ident}`.
    /// The slice is the variable name (may be empty for positional `{}`).
    VarRef(&'a str),
    /// A variable reference with format spec: `{ident:spec}`.
    /// First slice is the variable name, second is the spec including `:`.
    VarRefFormat(&'a str, &'a str),
}

/// Zero-allocation iterator over parts of a `format!`-style string.
///
/// All returned string slices reference the original input.
pub(crate) struct FormatIter<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> FormatIter<'a> {
    pub fn new(input: &'a str) -> Self {
        FormatIter { input, pos: 0 }
    }
}

impl<'a> Iterator for FormatIter<'a> {
    type Item = FormatPart<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.input.as_bytes();
        if self.pos >= bytes.len() {
            return None;
        }

        let start = self.pos;

        match bytes[self.pos] {
            b'{' => {
                self.pos += 1;
                if self.pos < bytes.len() && bytes[self.pos] == b'{' {
                    self.pos += 1;
                    return Some(FormatPart::EscapedBrace(&self.input[start..self.pos]));
                }
                let content_start = self.pos;
                let mut colon_pos = None;
                while self.pos < bytes.len() && bytes[self.pos] != b'}' {
                    if bytes[self.pos] == b':' && colon_pos.is_none() {
                        colon_pos = Some(self.pos);
                    }
                    self.pos += 1;
                }
                let content_end = self.pos;
                if self.pos < bytes.len() {
                    self.pos += 1; // skip closing '}'
                }
                match colon_pos {
                    Some(cp) => Some(FormatPart::VarRefFormat(
                        &self.input[content_start..cp],
                        &self.input[cp..content_end],
                    )),
                    None => Some(FormatPart::VarRef(&self.input[content_start..content_end])),
                }
            }
            b'}' => {
                self.pos += 1;
                if self.pos < bytes.len() && bytes[self.pos] == b'}' {
                    self.pos += 1;
                    return Some(FormatPart::EscapedBrace(&self.input[start..self.pos]));
                }
                // Lone '}' — treat as raw text
                Some(FormatPart::RawString(&self.input[start..self.pos]))
            }
            _ => {
                while self.pos < bytes.len() && bytes[self.pos] != b'{' && bytes[self.pos] != b'}' {
                    self.pos += 1;
                }
                Some(FormatPart::RawString(&self.input[start..self.pos]))
            }
        }
    }
}

/// Returns true if `name` is a valid captured identifier for `turbofmt!`.
///
/// Must start with an alphabetic character or `_`, and contain only
/// alphanumeric characters or `_`. Empty strings and numeric-only
/// names (positional args) return false.
fn is_captured_ident(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => chars.all(|c| c.is_alphanumeric() || c == '_'),
        _ => false,
    }
}

/// Extract captured variable names from a format string.
///
/// Only extracts plain `{ident}` captures (no format spec). Variables with
/// format specs like `{ident:?}` or `{ident:>10}` are left as-is — they may
/// use `Debug` or other traits that `ValueToStringify` doesn't handle.
fn extract_captured_variables(fmt: &str) -> Vec<String> {
    let mut vars = Vec::new();
    for part in FormatIter::new(fmt) {
        if let FormatPart::VarRef(name) = part {
            if is_captured_ident(name) && !vars.iter().any(|v: &String| v == name) {
                vars.push(name.to_string());
            }
        }
    }
    vars
}

/// Generate resolve statements for a list of expressions.
///
/// Each expression gets resolved via `ValueToStringify::to_stringify().await?`
/// into a local variable `__arg0`, `__arg1`, etc.
///
/// When `add_ref` is true, expressions are passed as `&(expr)` (for owned values).
/// When false, expressions are passed directly (for already-referenced values).
pub(crate) fn generate_resolve_stmts(exprs: &[Expr], add_ref: bool) -> Vec<TokenStream2> {
    exprs
        .iter()
        .enumerate()
        .map(|(i, expr)| {
            let var = format_ident!("__arg{}", i);
            if add_ref {
                quote! {
                    let #var = turbo_tasks::display::ValueToStringify::to_stringify(&(#expr)).await?;
                }
            } else {
                quote! {
                    let #var = turbo_tasks::display::ValueToStringify::to_stringify(#expr).await?;
                }
            }
        })
        .collect()
}

/// Generate variable identifiers `__arg0`, `__arg1`, etc. for a given count.
pub(crate) fn generate_arg_vars(count: usize) -> Vec<syn::Ident> {
    (0..count).map(|i| format_ident!("__arg{}", i)).collect()
}

/// Generate shadow bindings that resolve captured format string variables via
/// `ValueToStringify`.
///
/// For each captured variable like `{request}` in the format string, generates:
/// ```ignore
/// let request = ValueToStringify::to_stringify(&request).await?;
/// ```
/// This shadows the original binding with a resolved `String`, so `format!()`
/// can use it via `Display` even if the original type (e.g. `Vc<T>`) doesn't
/// implement `Display`.
fn generate_captured_resolve_stmts(captured_vars: &[String], add_ref: bool) -> Vec<TokenStream2> {
    captured_vars
        .iter()
        .map(|name| {
            let ident = format_ident!("{}", name);
            if add_ref {
                quote! {
                    let #ident = turbo_tasks::display::ValueToStringify::to_stringify(&#ident).await?;
                }
            } else {
                quote! {
                    let #ident = turbo_tasks::display::ValueToStringify::to_stringify(#ident).await?;
                }
            }
        })
        .collect()
}

/// Core code generation for `turbofmt!`-style formatting.
///
/// Given a format string and expressions, generates code that:
/// 1. Calls `ValueToStringify::to_stringify(&expr).await?` on each argument
/// 2. Passes the resolved values to `format!(fmt, __arg0, __arg1, ...)`
///
/// Returns a `TokenStream2` that evaluates to `impl Future<Output = Result<RcStr>>`.
pub(crate) fn generate_turbofmt(fmt: &str, exprs: &[Expr]) -> TokenStream2 {
    generate_turbofmt_inner(fmt, exprs, true)
}

/// Like `generate_turbofmt` but with control over whether `&` is added to expressions.
pub(crate) fn generate_turbofmt_inner(fmt: &str, exprs: &[Expr], add_ref: bool) -> TokenStream2 {
    let captured_vars = extract_captured_variables(fmt);
    let captured_stmts = generate_captured_resolve_stmts(&captured_vars, add_ref);
    let resolve_stmts = generate_resolve_stmts(exprs, add_ref);
    let vars = generate_arg_vars(exprs.len());

    if vars.is_empty() {
        quote! {
            async {
                #(#captured_stmts)*
                anyhow::Ok(turbo_rcstr::RcStr::from(format!(#fmt)))
            }
        }
    } else {
        quote! {
            async {
                #(#captured_stmts)*
                #(#resolve_stmts)*
                anyhow::Ok(turbo_rcstr::RcStr::from(format!(#fmt, #(#vars),*)))
            }
        }
    }
}

fn parse_fmt_args(input: TokenStream) -> syn::Result<(String, Vec<Expr>)> {
    let args: Punctuated<Expr, Token![,]> =
        syn::parse::Parser::parse(Punctuated::parse_terminated, input)?;
    let mut iter = args.into_iter();

    let first = iter
        .next()
        .ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "expected format string"))?;

    let fmt = match &first {
        Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) => s.value(),
        _ => {
            return Err(syn::Error::new_spanned(
                first,
                "first argument must be a format string literal",
            ));
        }
    };

    let exprs: Vec<Expr> = iter.collect();
    Ok((fmt, exprs))
}

/// `turbofmt!("format string {}", expr1, expr2)` — async format with `ValueToStringify`.
///
/// Returns a future that resolves each expression via `ValueToStringify::to_stringify().await?`
/// and then passes the resolved values to `format!()`.
///
/// Returns `impl Future<Output = Result<RcStr>>`. Must be `.await`ed.
pub fn turbofmt(input: TokenStream) -> TokenStream {
    match parse_fmt_args(input) {
        Ok((fmt, exprs)) => generate_turbofmt(&fmt, &exprs).into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// `turbobail!("error: {}", expr)` — async bail with `ValueToStringify`.
///
/// Resolves each expression via `ValueToStringify::to_stringify().await?`
/// and then calls `anyhow::bail!()`. Has implicit `await` and return flow,
/// just like `bail!()`.
pub fn turbobail(input: TokenStream) -> TokenStream {
    match parse_fmt_args(input) {
        Ok((fmt, exprs)) => {
            let captured_vars = extract_captured_variables(&fmt);
            let captured_stmts = generate_captured_resolve_stmts(&captured_vars, true);
            let resolve_stmts = generate_resolve_stmts(&exprs, true);
            let vars = generate_arg_vars(exprs.len());

            let output = if vars.is_empty() {
                quote! {
                    {
                        #(#captured_stmts)*
                        #(#resolve_stmts)*
                        anyhow::bail!(#fmt)
                    }
                }
            } else {
                quote! {
                    {
                        #(#captured_stmts)*
                        #(#resolve_stmts)*
                        anyhow::bail!(#fmt, #(#vars),*)
                    }
                }
            };

            output.into()
        }
        Err(e) => e.to_compile_error().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_string_only() {
        let parts: Vec<_> = FormatIter::new("hello world").collect();
        assert_eq!(parts, vec![FormatPart::RawString("hello world")]);
    }

    #[test]
    fn simple_var_ref() {
        let parts: Vec<_> = FormatIter::new("{name}").collect();
        assert_eq!(parts, vec![FormatPart::VarRef("name")]);
    }

    #[test]
    fn var_ref_with_format_spec() {
        let parts: Vec<_> = FormatIter::new("{name:?}").collect();
        assert_eq!(parts, vec![FormatPart::VarRefFormat("name", ":?")]);
    }

    #[test]
    fn escaped_braces() {
        let parts: Vec<_> = FormatIter::new("{{escaped}}").collect();
        assert_eq!(
            parts,
            vec![
                FormatPart::EscapedBrace("{{"),
                FormatPart::RawString("escaped"),
                FormatPart::EscapedBrace("}}"),
            ]
        );
    }

    #[test]
    fn mixed_parts() {
        let parts: Vec<_> = FormatIter::new("hello {name}, you are {age:>3} years old").collect();
        assert_eq!(
            parts,
            vec![
                FormatPart::RawString("hello "),
                FormatPart::VarRef("name"),
                FormatPart::RawString(", you are "),
                FormatPart::VarRefFormat("age", ":>3"),
                FormatPart::RawString(" years old"),
            ]
        );
    }

    #[test]
    fn empty_var_ref() {
        let parts: Vec<_> = FormatIter::new("{}").collect();
        assert_eq!(parts, vec![FormatPart::VarRef("")]);
    }

    #[test]
    fn positional_var_ref() {
        let parts: Vec<_> = FormatIter::new("{0} and {1}").collect();
        assert_eq!(
            parts,
            vec![
                FormatPart::VarRef("0"),
                FormatPart::RawString(" and "),
                FormatPart::VarRef("1"),
            ]
        );
    }

    #[test]
    fn empty_format_spec() {
        // {name:} is valid in Rust format strings (default format)
        let parts: Vec<_> = FormatIter::new("{name:}").collect();
        assert_eq!(parts, vec![FormatPart::VarRefFormat("name", ":")]);
    }

    #[test]
    fn extract_captures_plain_idents_only() {
        let vars = extract_captured_variables("hello {name}, {age:?}, {_x}, {0}");
        assert_eq!(vars, vec!["name", "_x"]);
    }

    #[test]
    fn extract_captures_deduplicates() {
        let vars = extract_captured_variables("{x} and {x} and {y}");
        assert_eq!(vars, vec!["x", "y"]);
    }

    #[test]
    fn extract_captures_skips_escaped() {
        let vars = extract_captured_variables("{{not_a_var}} {real}");
        assert_eq!(vars, vec!["real"]);
    }
}
