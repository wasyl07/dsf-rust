//! Async version of `subscribe_object_model` (patch mode).
use dsf::asynchronous::AsyncSubscribeConnection;
use dsf::object_model::apply_patch;
use dsf::SubscriptionMode;

#[tokio::main]
async fn main() -> dsf::Result<()> {
    let mut conn = AsyncSubscribeConnection::new(SubscriptionMode::Patch, None);
    conn.connect_default().await?;

    let mut model = conn.get_object_model().await?;
    println!("{model}");

    for _ in 0..3 {
        let patch = conn.get_object_model_patch().await?;
        apply_patch(&mut model, &serde_json::from_str(&patch)?);
        println!("{patch}");
    }
    conn.close();
    Ok(())
}
