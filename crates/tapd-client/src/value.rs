use serde_json::Value;

use crate::{Error, Result};

/// Convert a scalar TAPD JSON value to a string without floating-point
/// conversion. TAPD IDs are returned as both JSON strings and integers.
pub fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

pub fn field_string(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(value_as_string)
}

/// Parse TAPD's common list response shape.
///
/// Both `[{"Story": {...}}]` and `[{...}]` are accepted. A single wrapped
/// object is also accepted so endpoint inconsistencies do not leak into every
/// command handler.
///
/// # Errors
///
/// Returns an error when the response shape cannot represent records or a
/// wrapper contains a value of the wrong type.
pub fn records(data: &Value, wrapper: &str) -> Result<Vec<Value>> {
    match data {
        Value::Array(items) => items
            .iter()
            .map(|item| unwrap_record(item, wrapper))
            .collect(),
        Value::Object(map) => {
            if let Some(inner) = map.get(wrapper) {
                match inner {
                    Value::Array(items) => items
                        .iter()
                        .map(|item| unwrap_record(item, wrapper))
                        .collect(),
                    Value::Object(_) => Ok(vec![inner.clone()]),
                    _ => Err(protocol(format!(
                        "wrapper {wrapper:?} does not contain an object or array"
                    ))),
                }
            } else {
                Ok(vec![data.clone()])
            }
        }
        Value::Null => Ok(Vec::new()),
        _ => Err(protocol(format!(
            "expected object or array for {wrapper}, got {data}"
        ))),
    }
}

/// Extract exactly one record from a TAPD response.
///
/// # Errors
///
/// Returns an error when the response cannot be parsed or contains anything
/// other than one record.
pub fn record(data: &Value, wrapper: &str) -> Result<Value> {
    let mut values = records(data, wrapper)?;
    if values.len() != 1 {
        return Err(protocol(format!(
            "expected one {wrapper} record, got {}",
            values.len()
        )));
    }
    Ok(values.remove(0))
}

fn unwrap_record(value: &Value, wrapper: &str) -> Result<Value> {
    let Value::Object(map) = value else {
        return Err(protocol(format!(
            "expected a {wrapper} object, got {value}"
        )));
    };
    Ok(map.get(wrapper).cloned().unwrap_or_else(|| value.clone()))
}

fn protocol(message: String) -> Error {
    Error::Protocol { message }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn preserves_large_numeric_ids_as_decimal_strings() {
        let value: Value = serde_json::from_str("1137916943001018636").unwrap();
        assert_eq!(
            value_as_string(&value).as_deref(),
            Some("1137916943001018636")
        );
    }

    #[test]
    fn unwraps_tapd_list_records() {
        let data = json!([
            {"Story": {"id": "1", "name": "one"}},
            {"Story": {"id": "2", "name": "two"}}
        ]);
        let parsed = records(&data, "Story").unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(field_string(&parsed[1], "id").as_deref(), Some("2"));
    }
}
