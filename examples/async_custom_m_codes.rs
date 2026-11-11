//! Async code interception of M1234, M5678 and M7722.
//!
//! `receive_code` is cancel-safe, so it can be raced against Ctrl-C with `tokio::select!`
//! without losing data.
use dsf::asynchronous::{AsyncCommandApi, AsyncInterceptConnection};
use dsf::{CodeType, InterceptOptions, InterceptionMode, MessageType};

#[tokio::main]
async fn main() -> dsf::Result<()> {
    let mut opts = InterceptOptions::new(InterceptionMode::Pre);
    opts.filters = Some(vec!["M1234".into(), "M5678".into(), "M7722".into()]);
    opts.debug = true;
    let mut conn = AsyncInterceptConnection::new(opts);
    conn.connect_default().await?;

    loop {
        let code = tokio::select! {
            code = conn.receive_code() => code?,
            _ = tokio::signal::ctrl_c() => {
                println!("Ctrl-C, closing");
                conn.close();
                return Ok(());
            }
        };
        match (code.code_type, code.major_number) {
            (CodeType::MCode, Some(1234)) => {
                if !conn.flush(code.channel).await? {
                    println!("Flush failed");
                    conn.cancel_code().await?;
                    continue;
                }
                println!("{code} {:?}", code.flags);
                conn.resolve_code(MessageType::Success, None).await?;
            }
            (CodeType::MCode, Some(5678)) => {
                conn.resolve_code(MessageType::Success, None).await?;
                conn.close();
                return Ok(());
            }
            (CodeType::MCode, Some(7722)) => {
                if !conn.flush(code.channel).await? {
                    println!("Flush failed");
                    conn.cancel_code().await?;
                    continue;
                }
                // The Python example runs `sudo shutdown +1` here; we only print.
                println!("Would run: sudo shutdown +1");
                conn.resolve_code(MessageType::Warning, Some("Shutting down SBC in 1min..."))
                    .await?;
            }
            _ => conn.ignore_code().await?,
        }
    }
}
