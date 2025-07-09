use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Display};
use tokio::task::JoinSet;

use chrono::NaiveDateTime;
use dxr::{TryFromValue, TryToValue, Value};

#[derive(Debug, Default)]
pub struct ParamTree {
    params: DashMap<String, ParamValue>,
    /// A map of (node_id, param_name) to their api_uri.
    param_subscriptions: DashMap<String, Vec<ParamSubscription>>,
}

impl Display for ParamTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "ParamTree:")?;
        for item in self.params.iter() {
            let key = item.key();
            let value = item.value();
            display_internal_value(value, key, f)?;
        }
        Ok(())
    }
}

fn display_internal_value(
    value: &ParamValue,
    prefix: &str,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    match value {
        ParamValue::Integer(i) => writeln!(f, "{prefix}: {}", i)?,
        ParamValue::Boolean(b) => writeln!(f, "{prefix}: {}", b)?,
        ParamValue::String(s) => writeln!(f, "{prefix}: \"{}\"", s)?,
        ParamValue::Double(d) => writeln!(f, "{prefix}: {}", d)?,
        ParamValue::DateTime(d) => writeln!(f, "{prefix}: {}", d)?,
        ParamValue::Base64(b) => writeln!(f, "{prefix}: {:?}", b)?,
        ParamValue::Array(a) => {
            writeln!(f, "{prefix}: {:?}", a)?;
        }
        ParamValue::Structure(hm) => {
            for (key, value) in hm.iter() {
                display_internal_value(value, &format!("{prefix}/{key}"), f)?;
            }
        }
    }

    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
enum ParamValue {
    Integer(i32),
    Boolean(bool),
    String(String),
    Double(f64),
    DateTime(NaiveDateTime),
    Base64(Vec<u8>),
    Array(Vec<ParamValue>),
    Structure(HashMap<String, ParamValue>),
}

impl TryFromValue for ParamValue {
    fn try_from_value(value: &Value) -> Result<Self, dxr::DxrError> {
        let value = if let Ok(i) = i32::try_from_value(&value) {
            ParamValue::Integer(i)
        } else if let Ok(b) = bool::try_from_value(&value) {
            ParamValue::Boolean(b)
        } else if let Ok(s) = String::try_from_value(&value) {
            ParamValue::String(s)
        } else if let Ok(d) = f64::try_from_value(&value) {
            ParamValue::Double(d)
        } else if let Ok(d) = NaiveDateTime::try_from_value(&value) {
            ParamValue::DateTime(d)
        } else if let Ok(b) = Vec::<u8>::try_from_value(&value) {
            ParamValue::Base64(b)
        } else if let Ok(a) = Vec::<ParamValue>::try_from_value(&value) {
            ParamValue::Array(a)
        } else if let Ok(hm) = HashMap::<String, ParamValue>::try_from_value(&value) {
            ParamValue::Structure(hm)
        } else {
            log::error!("{:?}", value);
            return Err(dxr::DxrError::invalid_data(format!(
                "Failed to convert value: {:?}",
                value
            )));
        };

        Ok(value)
    }
}

impl TryToValue for ParamValue {
    fn try_to_value(&self) -> Result<Value, dxr::DxrError> {
        match self {
            ParamValue::Integer(i) => Ok(Value::i4(*i)),
            ParamValue::Boolean(b) => Ok(Value::boolean(*b)),
            ParamValue::String(s) => Ok(Value::string(s.clone())),
            ParamValue::Double(d) => Ok(Value::double(*d)),
            ParamValue::DateTime(d) => Ok(Value::string(d.to_string())),
            ParamValue::Base64(b) => Ok(Value::base64(b.to_vec())),
            ParamValue::Array(a) => Ok(a
                .iter()
                .map(|v| v.try_to_value().unwrap())
                .collect::<Vec<_>>()
                .try_to_value()
                .unwrap()),
            ParamValue::Structure(hm) => Ok(hm
                .iter()
                .map(|(k, v)| (k.clone(), v.try_to_value().unwrap()))
                .collect::<HashMap<_, _>>()
                .try_to_value()
                .unwrap()),
        }
    }
}

impl ParamTree {
    pub fn get_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();

        for item in self.params.iter() {
            let prefix = format!("/{}", item.key());

            keys.extend(item.value().get_keys(&prefix));
        }
        keys
    }

    pub fn contains(&self, key: &str) -> Result<bool, String> {
        let key = key.trim_matches('/');
        if key == "" {
            return Ok(!self.params.is_empty());
        }

        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            if let Some(value) = self.params.get(&key_prefix) {
                return Ok(value.contains(&key_rest));
            }
        } else {
            return Ok(self.params.contains_key(key));
        }

        Ok(false)
    }

    pub fn set(&self, key: &str, value: Value) -> Result<(), String> {
        // We will take in a value and a key, and we will need to
        // 1. Convert the Value to a &[u8] slice, possibly using bincode
        // 2. Remove any existing subtrees that are children of the key
        // 3. Insert the key-value pair into the database
        // 4. Update the subscribers (ignore for now)
        let key = key.trim_matches('/');

        if key == "" {
            // root node so reset entire tree
            self.params.clear();

            if let Ok(value) = HashMap::<String, Value>::try_from_value(&value) {
                for (key, value) in value.into_iter() {
                    self.set(&key, value)?;
                }
            } else {
                return Err(format!(
                    "invalid arguments: cannot set root of parameter tree to non-dictionary"
                ));
            }

            return Ok(());
        }

        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            if let Some(mut existing_value) = self.params.get_mut(&key_prefix) {
                existing_value.set(&key_rest, value)?;
            } else {
                let mut new_hm = ParamValue::Structure(HashMap::new());
                new_hm.set(&key_rest, value)?;
                self.params.insert(key_prefix, new_hm);
            }
        } else {
            self.params
                .insert(key.to_string(), ParamValue::try_from_value(&value).unwrap());
        }

        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<Value>, String> {
        // 1. We will need to get all values that have a prefix of the key
        // 2. We will need to decode the values
        // 3. We will need to convert those into a single Value
        let key = key.trim_matches('/');

        if key == "" {
            // root node so return entire tree as a hashmap
            let mut hm = HashMap::new();
            for item in self.params.iter() {
                hm.insert(item.key().clone(), item.value().try_to_value().unwrap());
            }
            return Ok(Some(hm.try_to_value().map_err(|e| e.to_string())?));
        }

        if let Some(value) = self.params.get(key) {
            return Ok(Some(value.try_to_value().map_err(|e| e.to_string())?));
        }

        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            if let Some(value) = self.params.get(&key_prefix) {
                return value.get(&key_rest);
            }
        }

        return Ok(None);
    }

    /// Search for parameter key on parameter server. Search starts in caller's namespace and proceeds upwards through parent namespaces until Parameter Server finds a matching key.
    ///
    /// searchParam's behavior is to search for the first partial match. For example, imagine that there are two 'robot_description' parameters:
    ///
    ///   /robot_description
    ///     /robot_description/arm
    ///     /robot_description/base
    ///   /pr2/robot_description
    ///     /pr2/robot_description/base
    /// If I start in the namespace /pr2/foo and search for 'robot_description', searchParam will match /pr2/robot_description. If I search for 'robot_description/arm' it will return /pr2/robot_description/arm, even though that parameter does not exist (yet).
    pub fn search(&self, caller_id: &str, key: &str) -> Result<Option<Value>, String> {
        let caller_id = caller_id.trim_matches('/');
        let key = key.trim_matches('/');

        let mut namespace = caller_id.split('/').collect::<Vec<&str>>();
        let first_key = key.split('/').next().unwrap_or(key);

        loop {
            let param = namespace.join("/");
            let param_name = format!("{param}/{first_key}");

            if let Ok(true) = self.contains(&param_name) {
                let full_name = format!("/{param}/{key}");
                return Ok(Some(full_name.try_to_value().map_err(|e| e.to_string())?));
            }

            if namespace.pop().is_none() {
                break;
            }
        }

        Ok(None)
    }

    pub fn delete(&self, key: &str) -> bool {
        // 1. We will need to delete all entries that have a prefix of the key
        let key = key.trim_matches('/');

        if self.params.remove(key).is_some() {
            return true;
        }

        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            if let Some(mut value) = self.params.get_mut(&key_prefix) {
                value.delete(&key_rest);
                return true;
            }
        }

        return false;

        // self.update_subscribers(key, caller_id).await;
    }

    pub fn subscribe(
        &self,
        caller_id: String,
        param: String,
        api_uri: String,
    ) -> Result<Option<Value>, String> {
        // replace old entry if subscribing node has restarted
        if let Some(mut existing_param_sub) = self.param_subscriptions.get_mut(&param) {
            if let Some(existing_api_uri) = existing_param_sub
                .iter_mut()
                .find(|s| s.node_id == caller_id)
            {
                if existing_api_uri.api_uri != api_uri {
                    // URI needs updated
                    existing_api_uri.api_uri = api_uri.clone();
                } else {
                    log::debug!("{} is already subscribed to {}", caller_id, param);
                }
            } else {
                // A new node is subscribing to this param
                existing_param_sub.push(ParamSubscription {
                    node_id: caller_id.clone(),
                    api_uri,
                });
            }
        } else {
            // A new param is being subscribed to
            self.param_subscriptions.insert(
                param.clone(),
                vec![ParamSubscription {
                    node_id: caller_id.clone(),
                    api_uri,
                }],
            );
        }

        self.get(&param)
    }

    /// Returns true if the subscription was removed, false if it was not found.
    pub fn unsubscribe(&self, caller_id: String, param: String, _api_uri: String) -> bool {
        let mut removed = false;
        let mut needs_cleanup = false;

        if let Some(mut existing_param_sub) = self.param_subscriptions.get_mut(&param) {
            if let Some(_existing_api_uri) = existing_param_sub
                .iter_mut()
                .find(|s| s.node_id == caller_id)
            {
                existing_param_sub.retain(|s| s.node_id != caller_id);
                if existing_param_sub.is_empty() {
                    needs_cleanup = true;
                }

                removed = true;
            }
        }

        if needs_cleanup {
            self.param_subscriptions.remove(&param);
        }

        removed
    }

    pub async fn update_subscribers(&self, key: &str, caller_id: String) {
        let mut update_futures = JoinSet::new();
        let key = key.trim_matches('/');

        for subscription in self.param_subscriptions.iter() {
            if subscription.key().starts_with(key) {
                let value = self.get(subscription.key()).unwrap();
                if let Some(new_value) = value {
                    for node in subscription.value().iter() {
                        update_futures.spawn(update_client_with_new_param_value(
                            node.api_uri.clone(),
                            caller_id.clone(),
                            node.node_id.clone(),
                            subscription.key().to_string(),
                            ParamValue::try_from_value(&new_value).unwrap(),
                        ));
                    }
                } else {
                    log::warn!(
                        "Parameter {} no longer exists, skipping update for subscriber {:?}",
                        subscription.key(),
                        subscription
                            .value()
                            .iter()
                            .map(|s| s.node_id.clone())
                            .collect::<Vec<_>>()
                    );
                }
            }
        }

        while let Some(res) = update_futures.join_next().await {
            match res {
                // Ok(Ok(v)) => {
                //     log::debug!("a subscriber has been updated (res: {:#?})", &v);
                // }
                // Ok(Err(err)) => {
                //     log::warn!(
                //         "Error updating a subscriber of changed param {}:\n{:#?}",
                //         &key,
                //         err
                //     );
                // }
                Err(err) => {
                    log::warn!(
                        "Error updating a subscriber of changed param {}:\n{:#?}",
                        &key,
                        err
                    );
                }
                _ => (),
            }
        }
    }
}

impl ParamValue {
    fn contains(&self, key: &str) -> bool {
        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            match self {
                ParamValue::Structure(hm) => {
                    if let Some(value) = hm.get(&key_prefix) {
                        return value.contains(&key_rest);
                    } else {
                        return false;
                    }
                }
                _ => {
                    return false;
                }
            }
        } else {
            match self {
                ParamValue::Structure(hm) => {
                    return hm.contains_key(key);
                }
                _ => {
                    return false;
                }
            }
        }
    }

    fn get_keys(&self, prefix: &str) -> Vec<String> {
        let mut keys = Vec::new();
        match self {
            ParamValue::Structure(hm) => {
                for (key, value) in hm.iter() {
                    let new_prefix = format!("{prefix}/{key}");
                    keys.extend(value.get_keys(&new_prefix));
                }
            }
            _ => {
                keys.push(prefix.to_owned());
            }
        }
        keys
    }

    /// Recursively insert a value into the database.
    /// If the value is a HashMap, we will insert each leaf node into the database.
    /// If the value is not a HashMap, we will insert the value into the database.
    fn set(&mut self, key: &str, value_to_insert: Value) -> Result<(), String> {
        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            // Check if existing value is a HashMap
            match self {
                ParamValue::Structure(hm) => {
                    if let Some(internal_value) = hm.get_mut(&key_prefix) {
                        internal_value.set(&key_rest, value_to_insert)?;
                    } else {
                        let mut new_hm = ParamValue::Structure(HashMap::new());
                        new_hm.set(&key_rest, value_to_insert)?;
                        hm.insert(key_prefix, new_hm);
                    }
                }
                _ => {
                    let mut new_hm = ParamValue::Structure(HashMap::new());
                    new_hm.set(&key_rest, value_to_insert)?;
                    *self = new_hm;
                }
            }
        } else {
            match self {
                ParamValue::Structure(hm) => {
                    hm.insert(
                        key.to_string(),
                        ParamValue::try_from_value(&value_to_insert).unwrap(),
                    );
                }
                _ => {
                    let mut new_hm = HashMap::new();
                    new_hm.insert(
                        key.to_string(),
                        ParamValue::try_from_value(&value_to_insert).unwrap(),
                    );
                    *self = ParamValue::Structure(new_hm);
                }
            }
        }

        Ok(())
    }

    /// Returns the value at the given key, or None if the key does not exist.
    fn get(&self, key: &str) -> Result<Option<Value>, String> {
        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            match self {
                ParamValue::Structure(hm) => {
                    if let Some(value) = hm.get(&key_prefix) {
                        return value.get(&key_rest);
                    } else {
                        return Ok(None);
                    }
                }
                _ => {
                    return Ok(None);
                }
            }
        } else {
            match self {
                ParamValue::Structure(hm) => {
                    return Ok(hm.get(key).map(|v| v.try_to_value().ok()).flatten());
                }
                _ => {
                    return Ok(None);
                }
            }
        }
    }

    fn delete(&mut self, key: &str) {
        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            match self {
                ParamValue::Structure(hm) => {
                    if let Some(internal_value) = hm.get_mut(&key_prefix) {
                        return internal_value.delete(&key_rest);
                    } else {
                        return;
                    }
                }
                _ => {
                    return;
                }
            }
        } else {
            match self {
                ParamValue::Structure(hm) => {
                    hm.remove(key);
                    return;
                }
                _ => {
                    return;
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct ParamSubscription {
    node_id: String,
    api_uri: String,
}

async fn update_client_with_new_param_value(
    _client_api_url: String,
    _updating_node_id: String,
    _subscribing_node_id: String,
    param_name: String,
    new_value: ParamValue,
) -> Result<Value, anyhow::Error> {
    // let _client_api = ClientApi::new(&client_api_url);
    let param_value = new_value
        .try_to_value()
        .map_err(|e| anyhow::anyhow!("Failed to convert param value: {}", e))?;

    log::info!("paramUpdate[{}]", param_name);
    // TODO: remove this once we have a way to test the param update
    return Ok(param_value);

    // let request = client_api.param_update(&updating_node_id, &param_name, &param_value);
    // let res = request.await;
    // match res {
    //     Ok(ref v) => log::debug!(
    //         "Sent new value for param '{}' to node '{}'. response: {:?}",
    //         param_name,
    //         subscribing_node_id,
    //         &v
    //     ),
    //     Err(ref e) => log::debug!(
    //         "Error sending new value for param '{}' to node '{}': {:?}",
    //         param_name,
    //         subscribing_node_id,
    //         e
    //     ),
    // }

    // Ok(res?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_param_value() {
        let tree = ParamTree::default();
        tree.set("run_id", Value::string("asdf-jkl0".to_owned()))
            .unwrap();
        tree.set("robot_configs", Value::i4(23)).unwrap();
        tree.set("robot_configs/robot_speed", Value::double(3.0))
            .unwrap();
        tree.set("robot_configs/robot_id", Value::i4(24)).unwrap();
        tree.set("arms/arm_left/length", Value::double(-0.45))
            .unwrap();
        tree.set("arms/arm_right/length", Value::double(0.45))
            .unwrap();

        tree.set("robot_configs", Value::i4(23)).unwrap();
        let res: Value = tree.get("robot_configs").unwrap().unwrap();
        assert_eq!(res, Value::i4(23));

        println!("{}", tree);

        assert!(tree.contains("/").unwrap());
        assert!(tree.contains("/arms").unwrap());
        assert!(tree.contains("/arms/arm_left").unwrap());
    }

    #[test]
    fn test_param_tree_simple() {
        let run_id = Value::string("therunid".to_owned());
        let tree = ParamTree::default();
        tree.set("run_id", run_id.clone()).unwrap();

        // relative path or absolute path should work
        assert_eq!(tree.get("run_id").unwrap(), Some(run_id.clone()));
        assert_eq!(tree.get("/run_id").unwrap(), Some(run_id.clone()));
    }

    #[test]
    fn test_param_tree_set_get() {
        let tree = ParamTree::default();

        let param_value = Value::string("param_value".to_owned());
        tree.set("some/param", param_value.clone()).unwrap();

        println!("{}", tree);

        // relative path or absolute path should work
        assert_eq!(tree.get("some/param").unwrap(), Some(param_value.clone()));
        assert_eq!(tree.get("/some/param").unwrap(), Some(param_value.clone()));
    }

    #[test]
    fn test_param_tree_get_relative() {
        let tree = ParamTree::default();

        let param_value = Value::string("param_value".to_owned());
        tree.set("some/param", param_value.clone()).unwrap();
    }

    #[test]
    fn test_has_param() {
        let tree = ParamTree::default();
        tree.set("some/param", Value::string("param_value".to_owned()))
            .unwrap();
        assert!(tree.contains("some/param").unwrap());
        assert!(!tree.contains("some/param2").unwrap());
    }

    #[test]
    fn test_has_param_with_slash() {
        let tree = ParamTree::default();
        tree.set("some/param", Value::string("param_value".to_owned()))
            .unwrap();
        assert!(tree.contains("some/param/").unwrap());
        assert!(!tree.contains("some/param2/").unwrap());
    }

    #[test]
    fn test_has_search_param() {
        let tree = ParamTree::default();
        tree.set("robot_description/arm", Value::string("arm1".to_owned()))
            .unwrap();
        tree.set("robot_description/base", Value::string("base1".to_owned()))
            .unwrap();

        tree.set(
            "pr2/robot_description/base",
            Value::string("base2".to_owned()),
        )
        .unwrap();

        let res = tree
            .search("pr2/foo", "robot_description")
            .unwrap()
            .unwrap();
        assert_eq!(res, Value::string("pr2/robot_description".to_owned()));

        let res = tree
            .search("pr2/foo", "robot_description/arm")
            .unwrap()
            .unwrap();
        assert_eq!(res, Value::string("pr2/robot_description/arm".to_owned()));

        let res = tree
            .search("pr2/foo", "robot_description/base")
            .unwrap()
            .unwrap();
        assert_eq!(res, Value::string("pr2/robot_description/base".to_owned()));
    }

    #[test]
    fn test_param_tree_set_get_array() {
        let run_id = Value::string("therunid".to_owned());
        let tree = ParamTree::default();
        tree.set("run_id", run_id.clone()).unwrap();

        let param_value = vec![
            Value::string("param_value".to_owned()),
            Value::string("param_value2".to_owned()),
        ]
        .try_to_value()
        .unwrap();

        tree.set("some/param", param_value.clone()).unwrap();

        // relative path or absolute path should work
        assert_eq!(tree.get("some/param").unwrap(), Some(param_value.clone()));
        assert_eq!(tree.get("/some/param").unwrap(), Some(param_value.clone()));
    }

    #[test]
    fn test_param_tree_set_get_hashmap() {
        let tree = ParamTree::default();

        let param_value = HashMap::from([(
            "param_key".to_owned(),
            HashMap::from([(
                "param_key2".to_owned(),
                Value::string("param_value".to_owned()),
            )]),
        )])
        .try_to_value()
        .unwrap();

        tree.set("some/param", param_value.clone()).unwrap();

        println!("{}", tree);

        // relative path or absolute path should work
        assert_eq!(tree.get("some/param").unwrap(), Some(param_value.clone()));
        assert_eq!(tree.get("/some/param").unwrap(), Some(param_value.clone()));

        assert_eq!(
            tree.get("some/param/param_key/param_key2").unwrap(),
            Some(Value::string("param_value".to_owned()))
        );
        assert_eq!(
            tree.get("/some/param/param_key/param_key2").unwrap(),
            Some(Value::string("param_value".to_owned()))
        );
    }

    #[test]
    fn test_param_tree_set_get_hashmap_root() {
        let run_id = Value::string("therunid".to_owned());
        let tree = ParamTree::default();
        tree.set("run_id", run_id.clone()).unwrap();

        let param_tree = HashMap::from([(
            "param_key".to_owned(),
            HashMap::from([(
                "param_key2".to_owned(),
                Value::string("param_value".to_owned()),
            )]),
        )])
        .try_to_value()
        .unwrap();

        tree.set("/", param_tree.clone()).unwrap();

        println!("{}", tree);

        // relative path or absolute path should work
        assert_eq!(tree.get("/").unwrap(), Some(param_tree.clone()));
        assert_eq!(
            tree.get("/param_key").unwrap(),
            Some(
                HashMap::from([(
                    "param_key2".to_owned(),
                    Value::string("param_value".to_owned()),
                )])
                .try_to_value()
                .unwrap()
            )
        );
        assert_eq!(
            tree.get("/param_key/param_key2").unwrap(),
            Some(Value::string("param_value".to_owned()))
        );
    }

    fn create_complex_value() -> Value {
        HashMap::from([
            ("robot_id".to_owned(), Value::i4(42)),
            (
                "robot_configs".to_owned(),
                HashMap::from([
                    ("robot_speed".to_owned(), Value::double(3.0)),
                    ("robot_id".to_owned(), Value::i4(24)),
                ])
                .try_to_value()
                .unwrap(),
            ),
            (
                "sim_001".to_owned(),
                HashMap::from([
                    (
                        "arms".to_owned(),
                        HashMap::from([
                            (
                                "arm_left".to_owned(),
                                HashMap::from([
                                    ("length".to_owned(), Value::double(-0.45)),
                                    (
                                        "joints".to_owned(),
                                        HashMap::from([
                                            ("shoulder".to_owned(), Value::double(0.0)),
                                            ("elbow".to_owned(), Value::double(90.0)),
                                            ("wrist".to_owned(), Value::double(45.0)),
                                        ])
                                        .try_to_value()
                                        .unwrap(),
                                    ),
                                ])
                                .try_to_value()
                                .unwrap(),
                            ),
                            (
                                "arm_right".to_owned(),
                                HashMap::from([
                                    ("length".to_owned(), Value::double(0.45)),
                                    ("status".to_owned(), Value::string("active".to_owned())),
                                ])
                                .try_to_value()
                                .unwrap(),
                            ),
                        ])
                        .try_to_value()
                        .unwrap(),
                    ),
                    (
                        "sensors".to_owned(),
                        HashMap::from([
                            (
                                "camera".to_owned(),
                                HashMap::from([
                                    (
                                        "resolution".to_owned(),
                                        vec![Value::i4(1920), Value::i4(1080)]
                                            .try_to_value()
                                            .unwrap(),
                                    ),
                                    ("fps".to_owned(), Value::i4(30)),
                                ])
                                .try_to_value()
                                .unwrap(),
                            ),
                            (
                                "lidar".to_owned(),
                                HashMap::from([
                                    ("range".to_owned(), Value::double(100.0)),
                                    ("frequency".to_owned(), Value::double(10.0)),
                                ])
                                .try_to_value()
                                .unwrap(),
                            ),
                        ])
                        .try_to_value()
                        .unwrap(),
                    ),
                ])
                .try_to_value()
                .unwrap(),
            ),
        ])
        .try_to_value()
        .unwrap()
    }

    fn load_state() -> ParamTree {
        let tree = ParamTree::default();
        tree.set("run_id", Value::string("asdf-jkl0".to_owned()))
            .unwrap();
        tree.set("/", create_complex_value()).unwrap();

        tree
    }

    #[test]
    fn test_param_tree_get_keys() {
        let tree = load_state();
        let keys = tree.get_keys();
        println!("{}", tree);

        assert!(keys.contains(&"/sim_001/arms/arm_left/length".to_owned()));
        assert!(keys.contains(&"/sim_001/arms/arm_right/length".to_owned()));
        assert!(keys.contains(&"/sim_001/arms/arm_right/status".to_owned()));
        assert!(keys.contains(&"/sim_001/sensors/camera/resolution".to_owned()));
        assert!(keys.contains(&"/sim_001/sensors/camera/fps".to_owned()));
        assert!(keys.contains(&"/sim_001/sensors/lidar/range".to_owned()));
        assert!(keys.contains(&"/sim_001/sensors/lidar/frequency".to_owned()));
    }

    #[test]
    fn test_param_multiple_subscribers() {
        let tree = load_state();
        tree.subscribe(
            "node_1".to_string(),
            "param_key".to_string(),
            "http://node_1".to_string(),
        )
        .unwrap();
        tree.subscribe(
            "node_2".to_string(),
            "param_key".to_string(),
            "http://node_2".to_string(),
        )
        .unwrap();

        println!("{:#?}", tree.param_subscriptions);

        assert!(tree
            .param_subscriptions
            .get("param_key")
            .unwrap()
            .iter()
            .any(|s| s.node_id == "node_1"));
        assert!(tree
            .param_subscriptions
            .get("param_key")
            .unwrap()
            .iter()
            .any(|s| s.api_uri == "http://node_1"));
        assert!(tree
            .param_subscriptions
            .get("param_key")
            .unwrap()
            .iter()
            .any(|s| s.node_id == "node_2"));
        assert!(tree
            .param_subscriptions
            .get("param_key")
            .unwrap()
            .iter()
            .any(|s| s.api_uri == "http://node_2"));
    }
}
