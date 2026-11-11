//! Subscribe to the object model in patch mode and apply three patches.
use dsf::object_model::apply_patch;
use dsf::{SubscribeConnection, SubscriptionMode};

fn main() -> dsf::Result<()> {
    let mut conn = SubscribeConnection::new(SubscriptionMode::Patch, None);
    conn.connect_default()?;

    let mut model = conn.get_object_model()?;
    println!("{model}");

    for _ in 0..3 {
        let patch = conn.get_object_model_patch()?;
        apply_patch(&mut model, &serde_json::from_str(&patch)?);
        println!("{patch}");
    }
    conn.close();
    Ok(())
}
