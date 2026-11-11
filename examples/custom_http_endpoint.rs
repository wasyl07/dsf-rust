//! Create a custom GET endpoint at http://duet3/machine/custom/getIt
use dsf::{CommandApi, CommandConnection, HttpEndpointType, HttpResponseType};
use std::time::Duration;

fn main() -> dsf::Result<()> {
    let mut conn = CommandConnection::new().with_debug(true);
    conn.connect_default()?;

    let mut endpoint = conn.add_http_endpoint_default(HttpEndpointType::Get, "custom", "getIt")?;
    endpoint.set_endpoint_handler(|mut c| {
        if let Err(e) = c.read_request() {
            eprintln!("failed to read request: {e}");
            return;
        }
        let _ = c.send_response(
            200,
            "so happy you asked for it!",
            HttpResponseType::PlainText,
        );
    });

    println!("Try accessing http://duet3.local/machine/custom/getIt in your browser...");
    std::thread::sleep(Duration::from_secs(1800));

    endpoint.close();
    conn.close();
    Ok(())
}
