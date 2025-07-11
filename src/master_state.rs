extern crate dxr;
use std::collections::HashSet;

use dashmap::{DashMap, Entry};

use crate::client_api::ClientApi;

/// Struct containing information about ROS data.
#[derive(Default, Debug)]
pub struct MasterState {
    /// A map of service names to the set of nodes that provide that service.
    service_list: DashMap<String, DashMap<String, String>>,
    /// A map of node names to the API URL of the node.
    nodes: DashMap<String, String>,
    /// A map of topic names to the type of the topic.
    topics: DashMap<String, String>,
    /// A map of topic names to the set of nodes that are subscribed to that topic.
    subscriptions: DashMap<String, HashSet<String>>,
    /// A map of topic names to the set of nodes that are publishing to that topic.
    publications: DashMap<String, HashSet<String>>,
}

impl MasterState {
    pub fn register_node(&self, caller_id: &str, caller_api: &str) {
        match self.nodes.entry(caller_id.to_owned()) {
            Entry::Vacant(v) => {
                v.insert(caller_api.to_owned());
                log::debug!("New node registered: {caller_id} {caller_api}");
                return;
            }
            Entry::Occupied(mut e) => {
                let e = e.get_mut();
                if e == caller_api {
                    return;
                } else {
                    let old_api = std::mem::replace(e, caller_api.to_owned());
                    log::warn!(
                        "Node {caller_id} re-registered with different API: {old_api} -> {caller_api}"
                    );
                    self.cleanup_node(caller_id);
                }
            }
        }

        // let res = shutdown_node(&shutdown_api_url, caller_id).await;
        // if let Err(e) = res {
        //     log::warn!("Error shutting down previous instance of node '{caller_id}': {e:?}. New node will be registered regardless. Check for stray processes.");
        // }
    }

    pub fn register_service(
        &self,
        caller_id: &str,
        service: &str,
        service_api: &str,
        caller_api: &str,
    ) {
        self.register_node(&caller_id, &caller_api);
        self.service_list
            .entry(service.to_string())
            .or_default()
            .insert(caller_id.to_string(), service_api.to_string());
    }

    pub fn unregister_service(&self, caller_id: &str, service: &str, service_api: &str) -> bool {
        let service = resolve(&caller_id, &service);

        // need to manually check service api matches

        let removed = if let Some(providers) = self.service_list.get_mut(&service) {
            // Check if service API matches and remove provider
            let was_removed = providers
                .get(caller_id)
                .filter(|entry| entry.value() == service_api)
                .and(providers.remove(caller_id))
                .is_some();

            // Remove empty service entry
            if was_removed && providers.is_empty() {
                drop(providers);
                self.service_list.remove(&service);
            }

            was_removed
        } else {
            false
        };

        removed
    }

    /// Returns a list of XMLRPC API URIs for nodes currently publishing the
    /// specified topic.
    pub fn register_subscriber(
        &self,
        caller_id: &str,
        topic: &str,
        topic_type: &str,
        caller_api: &str,
    ) -> Vec<String> {
        let topic = resolve(&caller_id, &topic);
        self.register_node(&caller_id, &caller_api);

        if let Some(known_topic_type) = self.topics.get(&topic.clone()) {
            if known_topic_type.as_str() != topic_type && topic_type != "*" {
                log::warn!("Topic '{topic}' was initially published as '{known_topic_type:?}', but subscriber '{caller_id}' wants it as '{topic_type}'.");
            }
        }

        self.subscriptions
            .entry(topic.clone())
            .or_default()
            .insert(caller_id.to_string());

        let publishers = self
            .publications
            .get(&topic)
            .map(|p| p.clone())
            .unwrap_or_default();

        let publisher_apis: Vec<String> = publishers
            .iter()
            .filter_map(|p| self.nodes.get(p).map(|n| n.to_string()))
            .collect();

        publisher_apis
    }

    #[cfg(test)]
    /// Returns the name of the nodes that is subscribed to the given topic.
    fn lookup_subscriber(&self, caller_id: &str, topic: &str) -> Vec<String> {
        let topic = resolve(&caller_id, &topic);
        let subscribers = self.subscriptions.get(&topic).map(|s| s.clone());

        match subscribers {
            Some(subscribers) => subscribers.into_iter().collect(),
            None => vec![],
        }
    }

    pub fn unregister_subscriber(&self, caller_id: &str, topic: &str, caller_api: &str) -> bool {
        let topic = resolve(&caller_id, &topic);

        if !self.check_caller_api(caller_id, caller_api) {
            return false;
        }

        let removed = self
            .subscriptions
            .entry(topic)
            .or_default()
            .remove(caller_id);

        if removed {
            self.subscriptions.retain(|_, v| !v.is_empty());
        }

        removed
    }

    pub fn register_publisher(
        &self,
        caller_id: &str,
        topic: &str,
        topic_type: &str,
        caller_api: &str,
    ) -> Vec<String> {
        let topic = resolve(&caller_id, &topic);
        self.register_node(&caller_id, &caller_api);

        if let Some(existing_type) = self.topics.get(&topic.clone()) {
            if existing_type.as_str() != topic_type {
                log::warn!("New publisher for topic '{topic}' has type '{topic_type}', but it is already published as '{existing_type:?}'.");
            }
        }

        self.publications
            .entry(topic.clone())
            .or_default()
            .insert(caller_id.to_string());

        self.topics.insert(topic.clone(), topic_type.to_string());

        let subscribers_api_urls = self
            .subscriptions
            .get(&topic)
            .map(|s| s.clone())
            .unwrap_or_default()
            .iter()
            .filter_map(|s| self.nodes.get(s).map(|n| n.to_string()))
            .collect::<Vec<String>>();

        subscribers_api_urls
    }

    #[cfg(test)]
    /// Returns the name of the node that is publishing the given topic.
    fn lookup_publisher(&self, caller_id: &str, topic: &str) -> Vec<String> {
        let topic = resolve(&caller_id, &topic);
        let publishers = self.publications.get(&topic).map(|p| p.clone());

        match publishers {
            Some(publishers) => publishers.into_iter().collect(),
            None => vec![],
        }
    }

    /// TODO(mj): Make this private and create a background task to handle it.
    pub async fn publisher_update(
        &self,
        caller_id: &str,
        topic: &str,
        subscribers_api_urls: &[String],
    ) {
        let publishers = self
            .publications
            .get(topic)
            .map(|p| p.clone())
            .unwrap_or_default();

        // Inform all subscribers of the new publisher.
        let publisher_nodes = publishers.into_iter().collect::<Vec<String>>();
        let publisher_apis = self
            .nodes
            .iter()
            .filter(|node| publisher_nodes.contains(node.key()))
            .map(|node| node.value().clone())
            .collect::<Vec<String>>();

        for client_api_url in subscribers_api_urls {
            let client_api = ClientApi::new(&client_api_url);
            log::debug!("Call {}", client_api_url);
            log::info!(
                "publisherUpdate[{}] -> {} {:?}",
                topic,
                client_api_url,
                publisher_apis
            );
            let r = client_api
                .publisher_update(caller_id, &topic, &publisher_apis)
                .await;
            match r {
                Err(e) => log::warn!("publisherUpdate call to {} failed: {}", client_api_url, e),
                Ok(v) => {
                    log::info!(
                        "publisherUpdate[{}] -> {} {:?}: sec=0.01, result={:?}",
                        topic,
                        client_api_url,
                        publisher_apis,
                        v
                    );
                    log::debug!(
                        "publisherUpdate call to {} succeeded, returning: {:?}",
                        client_api_url,
                        v
                    );
                }
            }
        }
    }

    /// Unregisters a publisher node from the given topic.
    pub fn unregister_publisher(
        &self,
        caller_id: &str,
        topic: &str,
        caller_api: &str,
    ) -> Result<bool, String> {
        let topic = resolve(&caller_id, &topic);

        if !self.check_caller_api(caller_id, caller_api) {
            return Ok(false);
        }

        let removed = self
            .publications
            .entry(topic)
            .or_default()
            .remove(caller_id);
        if removed {
            self.publications.retain(|_, v| !v.is_empty());
        }

        Ok(removed)
    }

    /// Returns the API URL of the node with the given name.
    pub fn lookup_node(&self, node_name: &str) -> Option<String> {
        if let Some(node_api) = self.nodes.get(node_name) {
            return Some(node_api.to_string());
        } else {
            return None;
        }
    }

    /// Returns a list of topics that are published in the given subgraph and
    /// their types.
    pub fn get_published_topics(&self, caller_id: &str, subgraph: &str) -> Vec<(String, String)> {
        let subgraph = resolve(&caller_id, &subgraph);

        let mut result = Vec::<(String, String)>::new();
        for topic in self.publications.iter() {
            if !topic.key().starts_with(&subgraph) {
                continue;
            }

            let data_type = self.topics.get(topic.key());
            if let Some(data_type) = data_type {
                result.push((topic.key().to_string(), data_type.to_string()));
            }
        }
        result
    }

    /// Returns a list of topics and their types.
    pub fn get_topic_types(&self) -> Vec<(String, String)> {
        self.topics
            .iter()
            .map(|t| (t.key().to_string(), t.value().to_string()))
            .collect()
    }

    pub fn get_system_state(
        &self,
    ) -> (
        // Publishers
        Vec<(String, Vec<String>)>,
        // Subscribers
        Vec<(String, Vec<String>)>,
        // Services
        Vec<(String, Vec<String>)>,
    ) {
        let publishers: Vec<(String, Vec<String>)> = self
            .publications
            .iter()
            .map(|item| {
                let mut node_names: Vec<_> = item.value().iter().cloned().collect();
                node_names.sort();

                (item.key().to_string(), node_names)
            })
            .collect();
        let subscribers: Vec<(String, Vec<String>)> = self
            .subscriptions
            .iter()
            .map(|item| {
                let mut node_names: Vec<_> = item.value().iter().cloned().collect();
                node_names.sort();

                (item.key().to_string(), node_names)
            })
            .collect();
        let services: Vec<(String, Vec<String>)> = self
            .service_list
            .iter()
            .map(|item| {
                let mut node_names: Vec<_> = item
                    .value()
                    .iter()
                    .map(|internal_item| internal_item.key().to_string())
                    .collect::<Vec<String>>();
                node_names.sort();

                (item.key().to_string(), node_names)
            })
            .collect();
        (publishers, subscribers, services)
    }

    pub fn lookup_service(&self, caller_id: &str, service: &str) -> Result<String, String> {
        let service = resolve(&caller_id, &service);

        let services = self.service_list.get(&service).map(|s| s.clone());
        if let Some(services) = services {
            if let Some(entry) = services.iter().next() {
                return Ok(entry.value().clone());
            }
        }

        Err("no provider".to_string())
    }

    fn check_caller_api(&self, caller_id: &str, caller_api: &str) -> bool {
        match self.nodes.get(caller_id).map(|n| n.to_string()) {
            Some(known_caller_api) => known_caller_api == caller_api,
            None => false,
        }
    }

    /// Removes all references to the old node.
    fn cleanup_node(&self, caller_id: &str) {
        // Remove the node from any topic publication it is in
        self.publications.iter_mut().for_each(|mut v| {
            v.retain(|k| k != caller_id);
        });
        // Remove any empty publications
        self.publications.retain(|_, v| !v.is_empty());

        // Need to remove the node from any topic subscription it is in
        self.subscriptions.iter_mut().for_each(|mut v| {
            v.retain(|k| k != caller_id);
        });
        self.subscriptions.retain(|_, v| !v.is_empty());

        // Remove the node from any service it is in
        self.service_list.iter_mut().for_each(|mut v| {
            v.value_mut().retain(|k, _| k != caller_id);
        });
        self.service_list.retain(|_, v| !v.is_empty());
    }
}

async fn shutdown_node(client_api_url: &str, node_id: &str) -> anyhow::Result<()> {
    let client_api = ClientApi::new(client_api_url);
    let res = client_api
        .shutdown(
            "/master",
            &format!("[{}] Reason: new node registered with same name", node_id),
        )
        .await;
    res
}

fn resolve(caller_id: &str, key: &str) -> String {
    match key.chars().next() {
        None => "".to_owned(),
        Some('/') => key.to_owned(),
        Some('~') => format!("{}/{}", caller_id, &key[1..]),
        Some(_) => match caller_id.rsplit_once('/') {
            Some((namespace, _node_name)) => format!("{}/{}", namespace, key),
            None => key.to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_node() {
        let master_state = MasterState::default();
        master_state.register_node("node_1", "http://node_1");
        assert_eq!(
            master_state.lookup_node("node_1"),
            Some("http://node_1".to_string())
        );
    }

    #[test]
    fn test_register_service() {
        let master_state = MasterState::default();
        master_state.register_service("node_1", "service", "http://node_1", "http://node_1");
        assert_eq!(
            master_state.lookup_service("node_1", "service"),
            Ok("http://node_1".to_string())
        );
    }

    #[test]
    fn test_unregister_service() {
        let master_state = MasterState::default();
        master_state.register_service("node_1", "service", "http://node_1", "http://node_1");
        assert_eq!(
            master_state.unregister_service("node_1", "service", "http://node_1"),
            true
        );
    }

    #[test]
    fn test_register_subscriber() {
        let master_state = MasterState::default();
        master_state.register_subscriber("node_1", "topic", "std_msgs/String", "http://node_1");
        assert!(master_state
            .lookup_subscriber("node_1", "topic")
            .contains(&"node_1".to_string()),);
    }

    #[test]
    fn test_unregister_subscriber() {
        let master_state = MasterState::default();
        master_state.register_subscriber("node_1", "topic", "std_msgs/String", "http://node_1");
        assert_eq!(
            master_state.unregister_subscriber("node_1", "topic", "http://node_1"),
            true
        );
    }

    #[test]
    fn test_register_publisher() {
        let master_state = MasterState::default();
        master_state.register_publisher("node_1", "topic", "std_msgs/String", "http://node_1");
        assert!(master_state
            .lookup_publisher("node_1", "topic")
            .contains(&"node_1".to_string()),);
    }

    #[test]
    fn test_unregister_publisher() {
        let master_state = MasterState::default();
        master_state.register_publisher("node_1", "topic", "std_msgs/String", "http://node_1");
        assert_eq!(
            master_state.unregister_publisher("node_1", "topic", "http://node_1"),
            Ok(true)
        );
    }

    #[test]
    fn test_get_published_topics() {
        let master_state = MasterState::default();
        master_state.register_publisher("node_1", "topic", "std_msgs/String", "http://node_1");
        assert_eq!(
            master_state.get_published_topics("node_1", ""),
            vec![("topic".to_string(), "std_msgs/String".to_string())],
        );
    }

    #[test]
    fn test_get_topic_types() {
        let master_state = MasterState::default();
        master_state.register_publisher("node_1", "topic", "std_msgs/String", "http://node_1");
        assert_eq!(
            master_state.get_topic_types(),
            vec![("topic".to_string(), "std_msgs/String".to_string())],
        );
    }

    #[test]
    fn test_get_system_state() {
        let master_state = MasterState::default();
        master_state.register_publisher("node_1", "topic", "std_msgs/String", "http://node_1");
        assert_eq!(
            master_state.get_system_state(),
            (
                vec![("topic".to_string(), vec!["node_1".to_string()])],
                vec![],
                vec![]
            ),
        );
    }

    #[test]
    fn test_lookup_service() {
        let master_state = MasterState::default();
        master_state.register_service("node_1", "service", "http://node_1", "http://node_1");
        assert_eq!(
            master_state.lookup_service("node_1", "service"),
            Ok("http://node_1".to_string()),
        );
    }

    #[test]
    fn test_lookup_node() {
        let master_state = MasterState::default();
        master_state.register_node("node_1", "http://node_1");
        assert_eq!(
            master_state.lookup_node("node_1"),
            Some("http://node_1".to_string()),
        );
    }

    #[test]
    fn test_unregister_publisher_cleanup() {
        let master_state = MasterState::default();

        // Register 3 publishers
        master_state.register_publisher(
            "publisher_1",
            "/test_topic",
            "std_msgs/String",
            "http://LOCLAP858:38881/",
        );
        master_state.register_publisher(
            "publisher_2",
            "/test_topic",
            "std_msgs/String",
            "http://LOCLAP858:37635/",
        );
        master_state.register_publisher(
            "publisher_3",
            "/test_topic",
            "std_msgs/String",
            "http://LOCLAP858:39641/",
        );

        assert_eq!(
            master_state.lookup_publisher("test", "/test_topic").len(),
            3
        );

        // Unregister one publisher
        assert_eq!(
            master_state.unregister_publisher(
                "publisher_1",
                "/test_topic",
                "http://LOCLAP858:38881/",
            ),
            Ok(true)
        );

        // Register subscriber - should only see remaining 2 publishers
        let subscriber_apis = master_state.register_subscriber(
            "subscriber_1",
            "/test_topic",
            "std_msgs/String",
            "http://subscriber_1",
        );

        assert_eq!(subscriber_apis.len(), 2);

        let remaining: Vec<_> = subscriber_apis.into_iter().collect();
        assert!(remaining.contains(&"http://LOCLAP858:37635/".to_string()));
        assert!(remaining.contains(&"http://LOCLAP858:39641/".to_string()));
        assert!(!remaining.contains(&"http://LOCLAP858:38881/".to_string()));
    }

    #[test]
    fn test_port_mismatch_unregistration() {
        let master_state = MasterState::default();

        // Register publisher
        master_state.register_publisher(
            "speed_limiter_nodelets",
            "/expected_category",
            "std_msgs/Int8",
            "http://LOCLAP858:38881/",
        );

        let publishers = master_state.lookup_publisher("test", "/expected_category");
        assert_eq!(publishers.len(), 1);
        assert!(publishers.contains(&"speed_limiter_nodelets".to_string()));

        // Unregistration should fail (no port mismatch in current implementation)
        let removed = master_state.unregister_publisher(
            "speed_limiter_nodelets",
            "/expected_category",
            "http://LOCLAP858:38882/",
        );
        assert_eq!(removed, Ok(false));

        // Publisher should still be registered
        let publishers_after = master_state.lookup_publisher("test", "/expected_category");
        assert_eq!(publishers_after.len(), 1);

        // Subscriber should still see the publisher
        let subscriber_apis = master_state.register_subscriber(
            "subscriber_1",
            "/expected_category",
            "std_msgs/Int8",
            "http://subscriber_1",
        );

        assert_eq!(subscriber_apis.len(), 1);
        assert!(subscriber_apis.contains(&"http://LOCLAP858:38881/".to_string()));
    }

    #[test]
    fn test_node_reregistration_scenario() {
        let master_state = MasterState::default();

        // Register publishers
        master_state.register_publisher(
            "speed_limiter_nodelets",
            "/expected_category",
            "std_msgs/Int8",
            "http://LOCLAP858:38881/",
        );
        master_state.register_publisher(
            "locomotor",
            "/expected_category",
            "std_msgs/Int8",
            "http://LOCLAP858:37635/",
        );

        // Re-register node with different port
        master_state.register_node("speed_limiter_nodelets", "http://LOCLAP858:36917/");
        assert_eq!(
            master_state.lookup_node("speed_limiter_nodelets"),
            Some("http://LOCLAP858:36917/".to_string())
        );

        println!("Re-registered node: {master_state:#?}");

        // Automatically unregistered
        assert_eq!(
            master_state.unregister_publisher(
                "speed_limiter_nodelets",
                "/expected_category",
                "http://LOCLAP858:36917/",
            ),
            Ok(false)
        );

        println!("Re-registered node: {master_state:#?}");

        // Register subscriber - should only see remaining publisher
        let subscriber_apis = master_state.register_subscriber(
            "subscriber_1",
            "/expected_category",
            "std_msgs/Int8",
            "http://subscriber_1",
        );

        assert_eq!(subscriber_apis.len(), 1);
        let remaining: Vec<_> = subscriber_apis.into_iter().collect();
        assert!(remaining.contains(&"http://LOCLAP858:37635/".to_string()));
        assert!(!remaining.contains(&"http://LOCLAP858:38881/".to_string()));
        assert!(!remaining.contains(&"http://LOCLAP858:36917/".to_string()));
    }
}
