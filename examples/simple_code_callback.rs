//! Fire codes without blocking and get a callback when they are done.
use dsf::{CodeChannel, CommandConnection};
use std::time::Duration;

fn main() -> dsf::Result<()> {
    let mut conn = CommandConnection::new();
    conn.connect_default()?;

    conn.perform_simple_code_cb("G28", CodeChannel::Sbc, |r| println!("G28 done: {r:?}"))?;
    conn.perform_simple_code_cb("M115", CodeChannel::Sbc, |r| println!("M115 done: {r:?}"))?;

    // The calls above returned immediately, so main can keep working.
    for i in 0..3 {
        println!("main is still running ({i})");
        std::thread::sleep(Duration::from_millis(200));
    }

    // Don't exit before the callbacks have fired.
    conn.wait_pending();
    conn.close();
    Ok(())
}
