use std::{
    fmt::{self, Display},
    future::Future,
};

use anyhow::Result;
use turbo_rcstr::RcStr;
use turbo_tasks::Vc;
pub use turbo_tasks_macros::ValueToString;

use crate::{self as turbo_tasks, ReadRef, VcValueType, vc::ResolvedVc};

/// Converts a value to a string, like [`Display`], but returning `Vc<RcStr>`.
#[turbo_tasks::value_trait]
pub trait ValueToString {
    #[turbo_tasks::function]
    fn to_string(self: Vc<Self>) -> Vc<RcStr>;
}

/// Identity implementation: `RcStr` just returns itself, allowing `Vc<RcStr>`
/// and `ResolvedVc<RcStr>` to work with `turbofmt!` and `turbobail!`.
#[turbo_tasks::value_impl]
impl ValueToString for RcStr {
    #[turbo_tasks::function]
    fn to_string(&self) -> Vc<RcStr> {
        Vc::cell(self.clone())
    }
}

/// A helper trait used by the `#[derive(ValueToString)]` macro.
///
/// Provides async string conversion with a blanket implementation for `Display`
/// types. `Vc<T>` and `ResolvedVc<T>` have specialized implementations that
/// await the inner value's `ValueToString` implementation.
///
/// Note that these methods are inlined and these function calls are only used for
/// effecient macro codegen.
#[doc(hidden)]
pub trait ValueToStringify {
    fn to_stringify(self) -> impl Future<Output = Result<StringifyType>> + Send;
}

/// Used only for macro codegen.
#[doc(hidden)]
pub enum StringifyType {
    RcStr(ReadRef<RcStr>),
    String(String),
}

impl AsRef<str> for StringifyType {
    fn as_ref(&self) -> &str {
        match self {
            StringifyType::RcStr(s) => s.as_str(),
            StringifyType::String(s) => s.as_str(),
        }
    }
}

impl fmt::Debug for StringifyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_ref(), f)
    }
}

impl Display for StringifyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl From<StringifyType> for RcStr {
    fn from(s: StringifyType) -> Self {
        match s {
            StringifyType::RcStr(r) => (*r).clone(),
            StringifyType::String(s) => RcStr::from(s),
        }
    }
}

impl<T: Display + Send + Sync> ValueToStringify for &&T {
    #[inline(always)]
    fn to_stringify(self) -> impl Future<Output = Result<StringifyType>> + Send {
        let s = (*self).to_string();
        async move { Ok(StringifyType::String(s)) }
    }
}

/// Implementation for `Vc<T>` that awaits the turbo-tasks `ValueToString` result.
impl<T: Send> ValueToStringify for &Vc<T>
where
    T: ValueToString,
{
    #[inline(always)]
    fn to_stringify(self) -> impl Future<Output = Result<StringifyType>> + Send {
        let vc = *self;
        async move {
            let s = vc.to_string().await?;
            Ok(StringifyType::RcStr(s))
        }
    }
}

/// Implementation for `ResolvedVc<T>` that delegates to the `Vc<T>` implementation.
impl<T: Send> ValueToStringify for &ResolvedVc<T>
where
    T: ValueToString,
{
    #[inline(always)]
    fn to_stringify(self) -> impl Future<Output = Result<StringifyType>> + Send {
        let vc = *self;
        async move {
            let s = vc.to_string().await?;
            Ok(StringifyType::RcStr(s))
        }
    }
}

/// Implementation for `ReadRef<T>` that delegates to the `Vc<T>` implementation.
impl<T: Send> ValueToStringify for &ReadRef<T>
where
    T: ValueToString + VcValueType,
{
    #[inline(always)]
    async fn to_stringify(self) -> Result<StringifyType> {
        let s = ReadRef::<T>::cell(self.clone()).to_string().await?;
        Ok(StringifyType::RcStr(s))
    }
}
