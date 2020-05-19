use futures::pin_mut;
use futures::channel::mpsc;
use futures::prelude::*;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::prelude::*;
use tungstenite::protocol::Message;

type Tx = mpsc::UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

async fn handle_connection(peer_map: PeerMap, raw_stream: TcpStream, addr: SocketAddr) {
    println!("Incoming connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during websocket handshake");
    println!("WebSocket connection established: {}", addr);

    let (tx, rx) = mpsc::unbounded();
    peer_map.lock().unwrap().insert(addr, tx);

    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        println!("From {}: {}", addr, msg.to_text().unwrap());
        let peers = peer_map.lock().unwrap();

        let broadcast_recipients = peers
            .iter()
            .filter(|(peer_addr, _)| peer_addr != &&addr)
            .map(|(_, sink)| sink);
        for recp in broadcast_recipients {
            recp.unbounded_send(msg.clone()).unwrap();
        }

        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    peer_map.lock().unwrap().remove(&addr);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:6418".to_string());
    let state = PeerMap::new(Mutex::new(HashMap::new()));

    let sock = TcpListener::bind(&addr).await;
    let mut listener = sock.expect("Failed to bind");
    println!("Listening on: {}", addr);

    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(state.clone(), stream, addr));
    }

    Ok(())
}
