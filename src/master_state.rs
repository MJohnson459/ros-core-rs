extern crate dxr;
use std::collections::HashSet;

use dashmap::{DashMap, Entry};

use crate::client_api::ClientApi;

/// Struct containing information about ROS data.
#[derive(Default)]
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
        let shutdown_api_url;
        {
            match self.nodes.entry(caller_id.to_owned()) {
                Entry::Vacant(v) => {
                    v.insert(caller_api.to_owned());
                    return;
                }
                Entry::Occupied(mut e) => {
                    let e = e.get_mut();
                    if e == caller_api {
                        return;
                    } else {
                        shutdown_api_url = std::mem::replace(e, caller_api.to_owned());
                    }
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
        self.service_list
            .entry(service.to_string())
            .or_default()
            .insert(caller_id.to_string(), service_api.to_string());
        self.register_node(&caller_id, &caller_api);
    }

    pub fn unregister_service(&self, caller_id: &str, service: &str) -> bool {
        let service = resolve(&caller_id, &service);

        let removed = if let Some(providers) = self.service_list.get_mut(&service) {
            providers.remove(caller_id);
            providers.is_empty()
        } else {
            false
        };

        if removed {
            self.service_list.remove(&service);
        }

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

        if let Some(known_topic_type) = self.topics.get(&topic.clone()) {
            if known_topic_type.as_str() != topic_type && topic_type != "*" {
                log::warn!("Topic '{topic}' was initially published as '{known_topic_type:?}', but subscriber '{caller_id}' wants it as '{topic_type}'.");
            }
        }

        self.subscriptions
            .entry(topic.clone())
            .or_default()
            .insert(caller_id.to_string());

        self.register_node(&caller_id, &caller_api);

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

    pub fn unregister_subscriber(&self, caller_id: &str, topic: &str) -> bool {
        let topic = resolve(&caller_id, &topic);

        let removed = self
            .subscriptions
            .entry(topic.clone())
            .or_default()
            .remove(caller_id);

        self.subscriptions.retain(|_, v| !v.is_empty());

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

        if let Some(v) = self.topics.get(&topic.clone()) {
            if v.as_str() != topic_type {
                log::warn!("New publisher for topic '{topic}' has type '{topic_type}', but it is already published as '{v:?}'.");
            }
        }

        self.register_node(&caller_id, &caller_api);

        // TODO(patwie): Maybe holding the lock for a longer time?
        // let mut publications = self.data.publications.write().unwrap();
        self.publications
            .entry(topic.clone())
            .or_default()
            .insert(caller_id.to_string());

        // TODO(mj): If the topic already exists, we should probably error and not overwrite.
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

    pub fn unregister_publisher(&self, caller_id: &str, topic: &str) -> Result<bool, String> {
        let topic = resolve(&caller_id, &topic);

        if self.publications.get(&topic.clone()).is_none() {
            return Err(format!("[{caller_id}] is not a registered node"));
        }
        let removed = self
            .publications
            .entry(topic.clone())
            .or_default()
            .remove(caller_id);
        self.publications.retain(|_, v| !v.is_empty());

        Ok(removed)
    }

    pub fn lookup_node(&self, node_name: &str) -> Option<String> {
        if let Some(node_api) = self.nodes.get(node_name) {
            return Some(node_api.to_string());
        } else {
            return None;
        }
    }

    pub fn get_published_topics(&self, subgraph: &str) -> Vec<(String, String)> {
        let mut result = Vec::<(String, String)>::new();
        for topic in self.publications.iter() {
            if !topic.key().starts_with(subgraph) {
                continue;
            }

            let data_type = self.topics.get(topic.key());
            if let Some(data_type) = data_type {
                result.push((topic.key().to_string(), data_type.to_string()));
            }
        }
        result
    }

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
            if let Some(service_url) = services.iter().next() {
                return Ok(service_url.clone());
            }
        }

        Err("no provider".to_string())
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

fn get_node_id() -> Option<[u8; 6]> {
    let ip_link = std::process::Command::new("ip")
        .arg("link")
        .output()
        .ok()?
        .stdout;
    let ip_link = String::from_utf8_lossy(&ip_link);
    let mut next_is_mac = false;
    let mut mac = None;
    for element in ip_link.split_whitespace() {
        if next_is_mac {
            mac = Some(element);
            break;
        }
        if element == "link/ether" {
            next_is_mac = true;
        }
    }
    let mac = mac?;
    let mut all_ok = true;
    let mac: Vec<u8> = mac
        .split(':')
        .filter_map(|hex| {
            let res = u8::from_str_radix(hex, 16);
            all_ok &= res.is_ok();
            res.ok()
        })
        .collect();
    if !all_ok {
        return None;
    }
    let mac: [u8; 6] = mac.try_into().ok()?;
    Some(mac)
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
        assert_eq!(master_state.unregister_service("node_1", "service"), true);
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
        assert_eq!(master_state.unregister_subscriber("node_1", "topic"), true);
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
            master_state.unregister_publisher("node_1", "topic"),
            Ok(true)
        );
    }

    #[test]
    fn test_get_published_topics() {
        let master_state = MasterState::default();
        master_state.register_publisher("node_1", "topic", "std_msgs/String", "http://node_1");
        assert_eq!(
            master_state.get_published_topics(""),
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
}
