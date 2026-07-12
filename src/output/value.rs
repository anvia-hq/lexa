use serde_json::{json, Map, Value};

pub(super) fn base(tool: &str, summary: Value) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("tool".to_string(), Value::String(tool.to_string()));
    flatten_metadata(&mut map, summary);
    map
}

pub(super) fn flatten_metadata(map: &mut Map<String, Value>, metadata: Value) {
    let Value::Object(obj) = metadata else {
        insert_if_kept(map, "value", metadata);
        return;
    };
    for (key, value) in obj {
        insert_if_kept(map, &key, value);
    }
}

pub(super) fn object(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    let mut map = Map::new();
    for (key, value) in entries {
        if keep_value(&value) {
            map.insert(key.to_string(), value);
        }
    }
    Value::Object(map)
}

pub(super) fn array(items: impl IntoIterator<Item = Value>) -> Value {
    Value::Array(items.into_iter().collect())
}

pub(super) fn row(items: impl IntoIterator<Item = Value>) -> Value {
    Value::Array(items.into_iter().collect())
}

pub(super) fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}

pub(super) fn n(value: usize) -> Value {
    json!(value)
}

pub(super) fn cols(names: &[&str]) -> Value {
    array(names.iter().map(|name| s(*name)))
}

pub(super) fn pick(payload: &Value, keys: &[&str]) -> Value {
    let mut map = Map::new();
    for key in keys {
        insert_if_kept(&mut map, key, get(payload, key));
    }
    Value::Object(map)
}

pub(super) fn drop_false_defaults(value: Value) -> Value {
    let Value::Object(mut map) = value else {
        return value;
    };
    for key in [
        "compact",
        "paths_only",
        "regex",
        "scope",
        "transitive",
        "truncated",
    ] {
        if map.get(key).and_then(Value::as_bool) == Some(false) {
            map.remove(key);
        }
    }
    Value::Object(map)
}

pub(super) fn without_keys(payload: &Value, keys: &[&str]) -> Value {
    let Some(obj) = payload.as_object() else {
        return payload.clone();
    };
    let mut map = Map::new();
    for (key, value) in obj {
        if !keys.contains(&key.as_str()) {
            insert_if_kept(&mut map, key, value.clone());
        }
    }
    Value::Object(map)
}

pub(super) fn get(payload: &Value, key: &str) -> Value {
    payload.get(key).cloned().unwrap_or(Value::Null)
}

pub(super) fn insert_if_kept(map: &mut Map<String, Value>, key: &str, value: Value) {
    if keep_value(&value) {
        map.insert(key.to_string(), value);
    }
}

pub(super) fn keep_value(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        _ => true,
    }
}

pub(super) fn prune_empty_and_null(value: &mut Value) -> bool {
    match value {
        Value::Array(items) => {
            for item in &mut *items {
                if let Value::Object(_) = item {
                    prune_empty_and_null(item);
                }
            }
            !items.is_empty()
        }
        Value::Object(map) => {
            map.retain(|_, item| prune_empty_and_null(item));
            !map.is_empty()
        }
        _ => keep_value(value),
    }
}
