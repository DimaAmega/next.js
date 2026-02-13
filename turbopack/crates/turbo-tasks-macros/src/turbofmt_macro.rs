use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Expr, ExprLit, Lit, Token, punctuated::Punctuated};

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
    let resolve_stmts = generate_resolve_stmts(exprs, add_ref);
    let vars = generate_arg_vars(exprs.len());

    if vars.is_empty() {
        quote! {
            async move {
                anyhow::Ok(turbo_rcstr::RcStr::from(format!(#fmt)))
            }
        }
    } else {
        quote! {
            async move {
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
            let resolve_stmts = generate_resolve_stmts(&exprs, true);
            let vars = generate_arg_vars(exprs.len());

            let output = if vars.is_empty() {
                quote! {
                    {
                        #(#resolve_stmts)*
                        anyhow::bail!(#fmt)
                    }
                }
            } else {
                quote! {
                    {
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
