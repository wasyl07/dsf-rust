//! Async version of `send_simple_code`.
use dsf::asynchronous::{AsyncCommandApi, AsyncCommandConnection};
use dsf::CodeChannel;

#[tokio::main]
async fn main() -> dsf::Result<()> {
    let mut conn = AsyncCommandConnection::new().with_debug(true);
    conn.connect_default().await?;
    let res = conn
        .perform_simple_code("M115", CodeChannel::Sbc, false)
        .await;
    conn.close();
    println!("M115 is telling us: {}", res?);
    Ok(())
}
