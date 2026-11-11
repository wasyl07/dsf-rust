//! Send a simple code over a command connection.
use dsf::{CodeChannel, CommandApi, CommandConnection};

fn main() -> dsf::Result<()> {
    let mut conn = CommandConnection::new().with_debug(true);
    conn.connect_default()?;
    let res = conn.perform_simple_code("M115", CodeChannel::Sbc, false);
    conn.close();
    println!("M115 is telling us: {}", res?);
    Ok(())
}
