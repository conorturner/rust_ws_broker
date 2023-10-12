use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{Message, WebSocket};
use tokio::sync::{mpsc, broadcast};
use uuid::Uuid;
use futures::{StreamExt, SinkExt};
use futures::stream::{SplitStream, SplitSink};
use log::{info, error, debug};
use warp::{Rejection};
use warp::http::StatusCode;

use crate::routing::routing_table::{RTMutex};
use crate::error_handling::AuthRejection;

async fn buffer_to_host(mut rx: UnboundedReceiverStream<Result<Message, warp::Error>>, mut tx: SplitSink<WebSocket, Message>) {
    while let Some(result) = rx.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                error!("reading host buffer error {}", e);
                break;
            }
        };

        match tx.send(msg).await {
            Ok(_) => {
            }
            Err(_disconnected) => {
                info!("failed to send from host buffer to host");
                break;
            }
        }
    }
}


async fn host_to_buffer(mut rx: SplitStream<WebSocket>, send: broadcast::Sender<Message>) {
    while let Some(result) = rx.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                error!("error reading from host websocket: {}", e);
                break;
            }
        };

        match send.send(msg) {
            Ok(_) => {
            }
            Err(_disconnected) => {
                info!("failed host -> buffer");
                break;
            }
        };
    }

    info!("host disconnected from buffer");
}

pub async fn on_host_connect(websocket: WebSocket, host_id: Uuid, rt: RTMutex) {
    let (mut tx, rx) = websocket.split();

    // Create buffer channel for passing between threads
    let (ch_tx, ch_rx) = mpsc::unbounded_channel();
    let ch_rx = UnboundedReceiverStream::new(ch_rx);
    let (send, _recv) = broadcast::channel::<Message>(2048);

    {
        let mut rt = rt.lock().await;
        let connection_idx = rt.create_channel(host_id).await.unwrap(); // should handle overflow here
        rt.store_connection(connection_idx, Some((ch_tx.clone(), send.clone()))).await;
    }

    info!("added host to routing table: {}", host_id);

    // Send UUID back to host to display
    match tx.send(Message::text(format!("{}", host_id))).await {
        Ok(_) => info!("sent uuid back to host: {}", host_id),
        Err(_e) => error!("Couldn't respond uuid: {}", host_id),
    }

    futures::join!(
        buffer_to_host(ch_rx, tx),
        host_to_buffer(rx, send)
    );

    info!("host disconnected: {}", host_id);

    {
        let rtlk = rt.lock().await;
        rtlk.remove_channel(host_id).await;
    }
}


pub async fn host_connect_route((authorization, ws, rt): (String, warp::ws::Ws, RTMutex)) -> Result<impl warp::Reply, Rejection> {
    if authorization != "1234" {
        return Err(warp::reject::custom(AuthRejection { token: authorization }));
    }
    let uuid = Uuid::new_v4();
    info!("host_connect {}", uuid);
    Ok(ws.on_upgrade(move |websocket| on_host_connect(websocket, uuid, rt)))
}

pub async fn check_host_fn((uuid, rt): (Uuid, RTMutex)) -> Result<impl warp::Reply, Rejection> {
    if rt.lock().await.contains_channel(uuid).await {
        Ok(StatusCode::OK)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}