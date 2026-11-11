//! Helpers for the (untyped) object model.
//!
//! The object model is represented as a plain [`serde_json::Value`]. Use
//! [`apply_patch`] to merge patches received from a subscribe connection.

use serde_json::Value;

/// Apply a DSF object model patch to `target`.
///
/// * Objects are merged key by key, recursively (a `null` patch value sets `null`).
/// * Arrays are truncated to the patch length, merged element-wise (object with
///   object recursively, anything else replaced) and extended with extra items.
/// * Any other combination replaces the target with the patch.
pub fn apply_patch(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, pv) in p {
                apply_patch(t.entry(k.clone()).or_insert(Value::Null), pv);
            }
        }
        (Value::Array(t), Value::Array(p)) => {
            t.truncate(p.len());
            for (i, pv) in p.iter().enumerate() {
                match t.get_mut(i) {
                    Some(tv) if tv.is_object() && pv.is_object() => apply_patch(tv, pv),
                    Some(tv) => *tv = pv.clone(),
                    None => t.push(pv.clone()),
                }
            }
        }
        (t, p) => *t = p.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nested_merge_and_null() {
        let mut t = json!({"a":{"b":1,"c":2},"d":[1],"e":"x"});
        apply_patch(
            &mut t,
            &json!({"a":{"b":5,"n":{"deep":true}},"e":null,"f":1}),
        );
        assert_eq!(
            t,
            json!({"a":{"b":5,"c":2,"n":{"deep":true}},"d":[1],"e":null,"f":1})
        );
    }

    #[test]
    fn null_target_replaced_by_object() {
        let mut t = json!({"a":null});
        apply_patch(&mut t, &json!({"a":{"x":1}}));
        assert_eq!(t, json!({"a":{"x":1}}));
    }

    #[test]
    fn array_truncate() {
        let mut t = json!({"l":[1,2,3,4]});
        apply_patch(&mut t, &json!({"l":[9,8]}));
        assert_eq!(t, json!({"l":[9,8]}));
    }

    #[test]
    fn array_merge_objects_and_append() {
        let mut t = json!({"l":[{"a":1,"b":2},5]});
        apply_patch(&mut t, &json!({"l":[{"b":3},6,{"new":1}]}));
        assert_eq!(t, json!({"l":[{"a":1,"b":3},6,{"new":1}]}));
    }

    #[test]
    fn array_object_replaced_by_scalar() {
        let mut t = json!({"l":[{"a":1}]});
        apply_patch(&mut t, &json!({"l":[null]}));
        assert_eq!(t, json!({"l":[null]}));
    }

    #[test]
    fn type_change_replaces() {
        let mut t = json!({"a":[1,2]});
        apply_patch(&mut t, &json!({"a":{"x":1}}));
        assert_eq!(t, json!({"a":{"x":1}}));
    }
}
