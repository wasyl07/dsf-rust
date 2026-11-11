//! Intercept M1234, M5678 and M7722.
//!
//! A helper thread cancels the blocking `receive_code` after 60 seconds to show
//! `InterceptConnection::canceller`.
use dsf::{
    CodeType, CommandApi, Error, InterceptConnection, InterceptOptions, InterceptionMode,
    MessageType,
};
use std::time::Duration;

fn main() -> dsf::Result<()> {
    let mut opts = InterceptOptions::new(InterceptionMode::Pre);
    opts.filters = Some(vec!["M1234".into(), "M5678".into(), "M7722".into()]);
    opts.debug = true;
    let mut conn = InterceptConnection::new(opts);
    conn.connect_default()?;

    let canceller = conn.canceller();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(60));
        canceller.cancel();
    });

    loop {
        let code = match conn.receive_code() {
            Ok(c) => c,
            Err(Error::Cancelled) => {
                println!("Cancelled by helper thread, closing");
                conn.close();
                return Ok(());
            }
            Err(e) => {
                println!("Closing connection: {e}");
                conn.close();
                return Err(e);
            }
        };
        match (code.code_type, code.major_number) {
            (CodeType::MCode, Some(1234)) => {
                // Flush only if the action must be in sync with the machine.
                if !conn.flush(code.channel)? {
                    println!("Flush failed");
                    conn.cancel_code()?;
                    continue;
                }
                println!("{code} {:?}", code.flags);
                conn.resolve_code(MessageType::Success, None)?;
            }
            (CodeType::MCode, Some(5678)) => {
                conn.resolve_code(MessageType::Success, None)?;
                conn.close();
                return Ok(());
            }
            (CodeType::MCode, Some(7722)) => {
                if !conn.flush(code.channel)? {
                    println!("Flush failed");
                    conn.cancel_code()?;
                    continue;
                }
                // The Python example runs `sudo shutdown +1` here; we only print.
                println!("Would run: sudo shutdown +1");
                conn.resolve_code(MessageType::Warning, Some("Shutting down SBC in 1min..."))?;
            }
            _ => conn.ignore_code()?,
        }
    }
}
