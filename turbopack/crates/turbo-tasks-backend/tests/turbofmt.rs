#![feature(arbitrary_self_types)]
#![feature(arbitrary_self_types_pointers)]
#![allow(clippy::needless_return)] // tokio macro-generated code doesn't respect this

use turbo_rcstr::RcStr;
use turbo_tasks::{ValueToString, Vc, turbobail, turbofmt};
use turbo_tasks_testing::{Registration, register, run_once};

static REGISTRATION: Registration = register!();

// --- Test types ---

#[turbo_tasks::value(shared)]
#[derive(ValueToString)]
#[value_to_string("item {name} (count: {count})")]
struct FmtNamedFields {
    name: RcStr,
    count: u32,
}

#[turbo_tasks::value(shared)]
#[derive(ValueToString)]
#[value_to_string("constant-value")]
struct FmtConstantString;

// --- Tests ---

/// turbofmt! with plain Display types (no Vc).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_turbofmt_display_types() {
    run_once(&REGISTRATION, || async {
        let s: RcStr = turbofmt!("hello {} world {}", 42u32, "abc").await?;
        assert_eq!(&*s, "hello 42 world abc");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

/// turbofmt! with no arguments (just a format string).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_turbofmt_no_args() {
    run_once(&REGISTRATION, || async {
        let s: RcStr = turbofmt!("hello world").await?;
        assert_eq!(&*s, "hello world");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

/// turbofmt! with Vc types.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_turbofmt_vc_types() {
    run_once(&REGISTRATION, || async {
        let v: Vc<FmtNamedFields> = FmtNamedFields {
            name: "foo".into(),
            count: 7,
        }
        .cell();
        let s: RcStr = turbofmt!("value: {}", v).await?;
        assert_eq!(&*s, "value: item foo (count: 7)");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

/// turbofmt! with ResolvedVc types.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_turbofmt_resolved_vc() {
    run_once(&REGISTRATION, || async {
        let v = FmtConstantString.resolved_cell();
        let s: RcStr = turbofmt!("resolved: {}", v).await?;
        assert_eq!(&*s, "resolved: constant-value");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

/// turbofmt! with mixed Display and Vc types.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_turbofmt_mixed() {
    run_once(&REGISTRATION, || async {
        let v: Vc<FmtNamedFields> = FmtNamedFields {
            name: "bar".into(),
            count: 3,
        }
        .cell();
        let s: RcStr = turbofmt!("prefix {} vc {} suffix", 99u32, v).await?;
        assert_eq!(&*s, "prefix 99 vc item bar (count: 3) suffix");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

/// turbobail! produces an error with the formatted message.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_turbobail() {
    run_once(&REGISTRATION, || async {
        let v: Vc<FmtConstantString> = FmtConstantString.cell();

        let result: anyhow::Result<()> = async {
            turbobail!("error: {} with {}", 42u32, v);
            #[allow(unreachable_code)]
            Ok(())
        }
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.to_string(), "error: 42 with constant-value");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}
