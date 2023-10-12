use std::vec::Vec;
use std::collections::HashMap;
use tokio::sync::{RwLock, mpsc, broadcast, Mutex};
use std::sync::Arc;
use warp::ws::Message;
use uuid::Uuid;

type ConnectedHost = Option<(mpsc::UnboundedSender<Result<Message, warp::Error>>, broadcast::Sender<Message>)>;
type ConnectedHostMap = RwLock<HashMap<Uuid, usize>>;
type ConnectionStack = RwLock<Vec<usize>>;
type ConnectionHeap = Mutex<Vec<ConnectedHost>>;

pub struct RoutingTable {
    heap: Arc<ConnectionHeap>,
    stack: Arc<ConnectionStack>,
    map: Arc<ConnectedHostMap>,
}

pub type RTMutex = Arc<Mutex<RoutingTable>>;

impl RoutingTable {
    pub fn locked() -> RTMutex {
        Arc::new(Mutex::new(RoutingTable::new()))
    }

    pub fn new() -> Self {
        let socket_limit = 1000;
        let socket_idx: Vec<usize> = (0..socket_limit).collect();
        let empty_heap = vec![None; socket_limit];

        RoutingTable {
            heap: Arc::new(Mutex::new(empty_heap)), // big array of references to connections
            stack: Arc::new(ConnectionStack::new(socket_idx)), // stack of free connection slots
            map: Arc::new(ConnectedHostMap::default()), // mapping of uuids to connection slots
        }
    }

    pub async fn store_connection(&mut self, channel_idx: usize, connection: ConnectedHost) {
        self.heap.clone().lock().await[channel_idx] = connection.clone();
    }

    pub async fn get_connection(&mut self, channel_idx: usize) -> ConnectedHost {
        self.heap.clone().lock().await[channel_idx].clone()
    }

    pub async fn create_channel(&self, host_id: Uuid) -> Option<usize> {
        // lock stack and take a usize copy from it
        let connection_idx = { self.stack.clone().write().await.pop() };
        if connection_idx.is_none() {
            None
        } else {
            let connection_idx = connection_idx.unwrap();
            { self.map.write().await.insert(host_id, connection_idx); } // lock map and add
            Some(connection_idx)
        }
    }

    pub async fn contains_channel(&self, host_id: Uuid) -> bool {
        self.map.read().await.contains_key(&host_id)
    }

    pub async fn get_channel(&self, host_id: Uuid) -> Option<usize> {
        let map = self.map.read().await;
        let res = map.get(&host_id);

        if res.is_none() {
            None
        } else {
            Some(res.unwrap().clone())
        }
    }

    pub async fn remove_channel(&self, host_id: Uuid) {
        self.map.write().await.remove(&host_id);
    }
}