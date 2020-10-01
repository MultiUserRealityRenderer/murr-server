//use futures::channel::mpsc;
use futures::prelude::*;
use log::*;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use warp::ws::{Message, WebSocket};
use warp::Filter;

type Users = Arc<RwLock<HashMap<SocketAddr, mpsc::UnboundedSender<Result<Message, warp::Error>>>>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();

    // Default to an odd port for now
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "[::]:6418".to_string());
    println!("Listening on {}", addr);

    // Connected users state
    let users = Users::default();
    // turn it into a "filter"
    let users = warp::any().map(move || users.clone());

    // GET /ws -> WebSocket upgrade
    let ws = warp::path("ws")
        .and(warp::ws())
        .and(users)
        .and(warp::addr::remote())
        .map(|ws: warp::ws::Ws, users, address: Option<SocketAddr>| {
            // Will call the function if upgrade succeeds
            ws.on_upgrade(move |socket| user_connected(socket, users, address.unwrap()))
        });

    // replace w/ a redirect to static domain?
    // GET / -> static HTML
    let index = warp::path::end().map(|| warp::reply::html(INDEX_HTML));

    // Route aggregation
    let routes = index.or(ws);

    warp::serve(routes).run(addr.parse::<SocketAddr>()?).await;

    Ok(())
}

async fn user_connected(ws: WebSocket, users: Users, addr: SocketAddr) {
    // noop
    info!("New user: {}", addr);

    // Split the websocket
    let (user_tx, user_rx) = ws.split();

    // Create a channel to recieve messages from other users...
    let (tx, rx) = mpsc::unbounded_channel();

    // Spawn the task to move messages to the socket
    tokio::task::spawn(rx.forward(user_tx).map(|result| {
        if let Err(e) = result {
            info!("websocket send error: {}", e);
        }
    }));

    // Insert the user in the list of users
    users.write().await.insert(addr, tx);

    let result = user_rx
        .for_each(|msg| user_message(addr, msg.unwrap(), &users))
        .await;

    info!("user disconnected: {}, {:?}", addr, result);
    users.write().await.remove(&addr);
}

async fn user_message(my_addr: SocketAddr, msg: Message, users: &Users) {
    // Skip any non-Text messages...
    let msg = if let Ok(s) = msg.to_str() {
        s
    } else {
        return;
    };

    let new_msg = format!("<User#{}>: {}", my_addr, msg);

    // New message from this user, send it to everyone else
    for (&addr, tx) in users.read().await.iter() {
        if my_addr != addr {
            if let Err(_disconnected) = tx.send(Ok(Message::text(new_msg.clone()))) {
                // The tx is disconnected, our `user_disconnected` code
                // should be happening in another task, nothing more to
                // do here.
            }
        }
    }
}

static INDEX_HTML: &str = include_str!("index.html");
