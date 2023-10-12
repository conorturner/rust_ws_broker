use std::sync::Arc;
use tokio::sync::RwLock;
use std::net::IpAddr;
use std::env;
use dns_lookup::lookup_host;
use tokio::time::{sleep, Duration};
use log::{error, trace, Level, log_enabled};
use tokio::task;
use uuid::Uuid;
use warp::http::StatusCode;
use hyper::Client;

pub type Replicas = Arc<RwLock<Vec<IpAddr>>>;

pub async fn find_replica(replicas: Replicas, host_id: Uuid) -> Option<IpAddr> {
    let client = Client::new();

    for peer in replicas.read().await.iter() {
        let uri: String = format!("http://{}:8080/check_host/{}", peer, host_id).to_owned();

        if let Ok(uri) = uri.parse() {
            match client.get(uri).await {
                Ok(resp) => {
                    if resp.status() == StatusCode::OK {
                        return Some(peer.clone());
                    }
                }
                Err(e) => error!("{:?}", e)
            }
        }
    }

    None
}


pub async fn service_discovery(replicas: Replicas) {
    let key = "PEER_DISCOVERY_ADDRESS";
    match env::var(key) {
        Ok(hostname) => {
            loop {
                // block_in_place ensure the non-async operation of checking dns does not block other tasks
                match task::block_in_place(|| lookup_host(&hostname)) {
                    Ok(ips) => {
                        if log_enabled!(Level::Trace) {
                            trace!("found {} replicas", ips.len());
                            for peer in ips.iter() {
                                trace!("found replica {}", peer);
                            }
                        }
                        *replicas.write().await = ips;
                    }
                    Err(e) => error!("checking dns failed '{}' {:?}", hostname, e),
                }
                sleep(Duration::from_millis(10000)).await
            }
        }
        Err(_e) => error!("Couldn't read env var: {}", key),
    }
}