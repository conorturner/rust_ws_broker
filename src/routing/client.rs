use warp::ws::{Message, WebSocket};
use tokio::sync::{mpsc, broadcast};
use uuid::Uuid;
use futures::{StreamExt, SinkExt};
use futures::stream::{SplitStream, SplitSink};
use log::{info, error, debug, trace};
use tokio_tungstenite::{WebSocketStream, connect_async};
use tokio_tungstenite::tungstenite::protocol;
use std::net::IpAddr;
use url::Url;
use std::convert::Infallible;

use crate::routing::discovery::{find_replica, Replicas};
use crate::routing::routing_table::{RTMutex};

async fn buffer_to_client(host_rx: &broadcast::Sender<Message>, mut tx: SplitSink<WebSocket, Message>, session_id: Uuid) {
    let mut host_rx = host_rx.subscribe();

    while let Ok(result) = host_rx.recv().await {
        let msg = match result.to_str() {
            Ok(s) => s,
            Err(_e) => continue
        };

        let header = &msg[..36];
        let body = &msg[36..];

        let sid = Uuid::parse_str(header);

        if sid.unwrap() == session_id { // only forward messages relevant to this client
            if let Err(_disconnected) = tx.send(Message::text(body)).await {
                info!("failed to send from broadcast to client");
                break;
            }
        }
    }
}

async fn client_to_buffer(mut rx: SplitStream<WebSocket>, tx: &mpsc::UnboundedSender<Result<Message, warp::Error>>, session_id: Uuid) {
    while let Some(result) = rx.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                // This error occurs when the client does not disconnect gracefully (every time)
                // if the client crashes, no kill signal is sent to the child process, thus:
                // TODO: makes sense to forward something here to the client to tell it to shut down the process (if not already done).
                info!("Error reading from client={} {}", session_id, e);
                break;
            }
        };

        let msg = match msg.to_str() {
            Ok(msg) => {
                // parse message and add session_id as header before writing to buffer
                let mut header = session_id.to_string();
                header.push_str(&msg); // push body after uuid
                header
            }
            Err(_e) => {
                // TODO: figure out why this happens (is it just keep alive packets?)
                debug!("could not parse client message as string (probably keep alive)");
                continue;
            }
        };

        if let Err(_disconnected) = tx.send(Ok(Message::text( msg))) {
            debug!("failed to send from client to host buffer");
            break;
        }
    }
}

async fn to_replica(mut c_rx: SplitStream<WebSocket>, mut r_tx: SplitSink<WebSocketStream<tokio::net::TcpStream>, protocol::Message>) {
    while let Some(result) = c_rx.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                error!("websocket read error {}", e);
                break;
            }
        };

        let msg = match msg.to_str() {
            Ok(msg) => msg,
            Err(_e) => {
                info!("client triggered disconnect");
                break;
            }
        };

        if let Err(_disconnected) = r_tx.send(protocol::Message::text(format!("{}", msg))).await {
            info!("failed to send from client to replica WS");
            break;
        }
    }
}

async fn from_replica(mut r_rx: SplitStream<WebSocketStream<tokio::net::TcpStream>>, mut c_tx: SplitSink<WebSocket, Message>) {
    while let Some(result) = r_rx.next().await {
        let data = result.unwrap().to_string();
        if let Err(_disconnected) = c_tx.send(Message::text(data)).await {
            info!("failed to send from replica WS to client WS");
            break;
        }
    }
}

async fn forward_to_replica(websocket: WebSocket, uuid: Uuid, replica: IpAddr) {
    let url = Url::parse(
        &format!("ws://{}:8080/client/forward/{}", replica, uuid)).unwrap().to_string();

    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");

    let (r_tx, r_rx) = ws_stream.split();
    let (tx, rx) = websocket.split();

    debug!("forwarded {} to replica {}", uuid, replica);

    futures::join!(
        to_replica(rx, r_tx),
        from_replica(r_rx, tx)
    );

    debug!("closed {} on replica {}", uuid, replica);
}

pub async fn on_client_forward(websocket: WebSocket, uuid: Uuid, rt: RTMutex) {
    let heap_index = {
        let rt_lkd = rt.lock().await;
        rt_lkd.get_channel(uuid).await
    };

    if heap_index.is_some() {
        let (host_tx, host_rx) = {
            rt.lock().await.get_connection(heap_index.unwrap()).await.unwrap()
        };
        let (tx, rx) = websocket.split();
        let host_rx_clone = host_rx.clone();

        let session_id = Uuid::new_v4();
        info!("forwarding client session {} to local connection {}", session_id, uuid);

        futures::join!(
            buffer_to_client(&host_rx_clone, tx, session_id),
            client_to_buffer(rx, &host_tx, session_id)
        );

        debug!("replica disconnected from host {}", uuid);
    }
}

pub async fn on_client_connect(websocket: WebSocket, uuid: Uuid, replicas: Replicas, rt: RTMutex) {
    let heap_index = {
        let rt_lkd = rt.lock().await;
        rt_lkd.get_channel(uuid).await
    };

    if heap_index.is_some() {
        let (host_tx, host_rx) = {
            rt.lock().await.get_connection(heap_index.unwrap()).await.unwrap()
        };

        let (tx, rx) = websocket.split();
        let host_rx_clone = host_rx.clone();
        let session_id = Uuid::new_v4();

        info!("forwarding client session {} to local connection {}", session_id, uuid);

        futures::join!(
            buffer_to_client(&host_rx_clone, tx, session_id),
            client_to_buffer(rx, &host_tx, session_id)
        );

        debug!("client disconnected from host {}", uuid);
    } else {
        debug!("host {} not found locally, checking replicas", uuid);
        match find_replica(replicas, uuid).await {
            Some(replica) => {
                info!("host {} found in replica {}", uuid, replica);
                forward_to_replica(websocket, uuid, replica).await;
            }
            None => info!("host {} no replica found", uuid)
        }
    }
}

pub async fn client_connect_route((uuid, ws, replicas_clone, rt): (Uuid, warp::ws::Ws, Replicas, RTMutex)) -> Result<impl warp::Reply, Infallible> {
    debug!("client_connect {}", uuid);
    // TODO: check token and return Err() if no match, http response must then be sent by error handler function
    Ok(ws.on_upgrade(move |websocket| on_client_connect(websocket, uuid, replicas_clone, rt)))
}

pub async fn client_forward_route((uuid, ws, rt): (Uuid, warp::ws::Ws, RTMutex)) -> Result<impl warp::Reply, Infallible> {
    debug!("client_forward {}", uuid);
    Ok(ws.on_upgrade(move |websocket| on_client_forward(websocket, uuid, rt)))
}