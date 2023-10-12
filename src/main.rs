mod routing;
mod util;
mod error_handling;

use routing::routing_table::RoutingTable;
use routing::host;
use routing::client;
use routing::discovery;

use warp::{Filter, Rejection};
use std::{env};
use json_logger;
use log::LevelFilter;
use uuid::Uuid;
use util::get_port;

async fn health_check(replicas: discovery::Replicas) -> Result<impl warp::Reply, Rejection> {
    let peers = replicas.read().await.iter()
        .map(|ip| ip.to_string())
        .collect::<Vec<String>>();

    Ok(warp::reply::json(&peers))
}

#[tokio::main]
async fn main() {
    match env::var("LOG_JSON") {
        Ok(_) => json_logger::init("app", LevelFilter::Info).unwrap(),
        Err(_) => {
            env::set_var("RUST_LOG", "debug");
            pretty_env_logger::init()
        }
    }

    let replicas = discovery::Replicas::default();
    let rt = RoutingTable::locked();

    // Peer discovery
    tokio::spawn(discovery::service_discovery(replicas.clone()));

    let replicas_clone = replicas.clone();
    let add_replicas = warp::any().map(move || replicas_clone.clone());
    let health = warp::path("healthcheck")
        .and(add_replicas)
        .and_then(health_check);

    let rt_clone = rt.clone();
    let host_connect = warp::path!("host" / "connect")
        .and(warp::header::<String>("authorization"))
        .and(warp::ws())
        .map(move |authorization, ws| { (authorization, ws, rt_clone.clone()) })
        .and_then(host::host_connect_route);

    let replicas_clone = replicas.clone();
    let rt_clone = rt.clone();
    let client_connect = warp::path!("client" / "connect" / Uuid)
        .and(warp::ws())
        .map(move |uuid, ws| { (uuid, ws, replicas_clone.clone(), rt_clone.clone()) })
        .and_then(client::client_connect_route);

    let rt_clone = rt.clone();
    let client_forward = warp::path!("client" / "forward" / Uuid)
        .and(warp::ws())
        .map(move |uuid, ws| { (uuid, ws, rt_clone.clone()) })
        .and_then(client::client_forward_route);

    let rt_clone = rt.clone();
    let check_host = warp::path!("check_host" / Uuid)
        .map(move |uuid: Uuid| { (uuid, rt_clone.clone()) })
        .and_then(host::check_host_fn);

    let routes = health
        .or(host_connect)
        .or(check_host)
        .or(client_connect)
        .or(client_forward)
        .recover(error_handling::error_handler);


    warp::serve(routes)
        .run(([0, 0, 0, 0], get_port()))
        .await;
}

