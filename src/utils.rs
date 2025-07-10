use dxr::{TryFromValue, Value};

/// Formats a `Value` object in a human-readable way for logging purposes.
///
/// This function attempts to convert the `Value` to various known types and
/// formats them appropriately. Strings are quoted, numbers are displayed as-is,
/// booleans are shown as true/false, and complex types are formatted clearly.
///
/// # Arguments
///
/// * `value` - The `Value` object to format
///
/// # Returns
///
/// A formatted string representation of the value
pub fn format_value(value: &Value) -> String {
    // Try to convert to various types in order of preference

    // Try string first
    if let Ok(s) = String::try_from_value(value) {
        return format!("'{}'", s.replace(['\n', '\r'], ""));
    }

    // Try boolean
    if let Ok(b) = bool::try_from_value(value) {
        return b.to_string();
    }

    // Try i32
    if let Ok(i) = i32::try_from_value(value) {
        return i.to_string();
    }

    // Try i64 (only if the i8 feature is enabled)
    #[cfg(feature = "i8")]
    if let Ok(i) = i64::try_from_value(value) {
        return i.to_string();
    }

    // Try f64
    if let Ok(f) = f64::try_from_value(value) {
        return f.to_string();
    }

    // Try Vec<Value> (arrays)
    if let Ok(vec) = Vec::<Value>::try_from_value(value) {
        let formatted_elements: Vec<String> = vec.iter().map(format_value).collect();
        return format!("[{}]", formatted_elements.join(", "));
    }

    // Try HashMap<String, Value> (structs/objects)
    if let Ok(map) = std::collections::HashMap::<String, Value>::try_from_value(value) {
        let formatted_pairs: Vec<String> = map
            .iter()
            .map(|(k, v)| format!("'{}': {}", k, format_value(v)))
            .collect();
        return format!("{{{}}}", formatted_pairs.join(", "));
    }

    // Try Vec<u8> (base64)
    if let Ok(bytes) = Vec::<u8>::try_from_value(value) {
        return format!("<base64: {} bytes>", bytes.len());
    }

    // Try chrono::NaiveDateTime
    if let Ok(dt) = chrono::NaiveDateTime::try_from_value(value) {
        return format!("<datetime: {}>", dt);
    }

    // If all else fails, use the Debug implementation
    format!("{:?}", value)
}

/// Formats a slice of `Value` objects as a comma-separated string using `format_value`.
///
/// # Arguments
///
/// * `params` - The slice of `Value` objects to format
///
/// # Returns
///
/// A formatted string representation of the parameters
pub fn format_params(params: &[Value]) -> String {
    params
        .iter()
        .map(format_value)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use dxr::TryToValue;
    use maplit::hashmap;

    #[test]
    fn test_format_string() {
        let value = Value::string("hello world".to_string());
        assert_eq!(format_value(&value), "\"hello world\"");
    }

    #[test]
    fn test_format_boolean() {
        let value = Value::boolean(true);
        assert_eq!(format_value(&value), "true");

        let value = Value::boolean(false);
        assert_eq!(format_value(&value), "false");
    }

    #[test]
    fn test_format_integer() {
        let value = Value::i4(42);
        assert_eq!(format_value(&value), "42");

        let value = Value::i4(-123);
        assert_eq!(format_value(&value), "-123");
    }

    #[test]
    fn test_format_double() {
        let value = Value::double(3.14);
        assert_eq!(format_value(&value), "3.14");

        let value = Value::double(-2.5);
        assert_eq!(format_value(&value), "-2.5");
    }

    #[test]
    fn test_format_array() {
        let value = vec![
            Value::string("hello".to_string()),
            Value::i4(42),
            Value::boolean(true),
        ]
        .try_to_value()
        .unwrap();

        assert_eq!(format_value(&value), "[\"hello\", 42, true]");
    }

    #[test]
    fn test_format_struct() {
        let value = hashmap! {
            "name".to_string() => Value::string("test".to_string()),
            "count".to_string() => Value::i4(42),
            "active".to_string() => Value::boolean(true),
        }
        .try_to_value()
        .unwrap();

        // Note: HashMap iteration order is not guaranteed, so we need to be flexible
        let formatted = format_value(&value);
        assert!(formatted.contains("name: \"test\""));
        assert!(formatted.contains("count: 42"));
        assert!(formatted.contains("active: true"));
        assert!(formatted.starts_with("{"));
        assert!(formatted.ends_with("}"));
    }

    #[test]
    fn test_format_nested() {
        let value = hashmap! {
            "user".to_string() => hashmap! {
                "name".to_string() => Value::string("alice".to_string()),
                "scores".to_string() => vec![
                    Value::i4(95),
                    Value::i4(87),
                    Value::i4(92),
                ].try_to_value().unwrap(),
            }.try_to_value().unwrap(),
            "active".to_string() => Value::boolean(true),
        }
        .try_to_value()
        .unwrap();

        let formatted = format_value(&value);
        assert!(formatted.contains("user: {"));
        assert!(formatted.contains("name: \"alice\""));
        assert!(formatted.contains("scores: [95, 87, 92]"));
        assert!(formatted.contains("active: true"));
    }

    #[test]
    fn test_format_params() {
        let params = vec![
            Value::string("foo".to_string()),
            Value::i4(42),
            Value::boolean(false),
        ];
        assert_eq!(format_params(&params), "\"foo\", 42, false");
    }

    #[test]
    fn test_complex_value() {
        // [2025-07-09T12:23:40Z DEBUG ros_core_rs::core] registerPublisher[Value { value: String("/ln_rst_test_node") }, Value { value: String("/rosout") }, Value { value: String("rosgraph_msgs/Log") }, Value { value: String("http://LOCLAP858:41145/") }]
        let value = vec![
            "/ln_rst_test_node".to_string(),
            "/rosout".to_string(),
            "rosgraph_msgs/Log".to_string(),
            "http://LOCLAP858:41145/".to_string(),
        ]
        .try_to_value()
        .unwrap();

        assert_eq!(format_value(&value), "[\"/ln_rst_test_node\", \"/rosout\", \"rosgraph_msgs/Log\", \"http://LOCLAP858:41145/\"]");
    }
}
