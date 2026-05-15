// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

use axum::Router;
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::net::TcpListener;

mod influx;
mod prom;

struct State {
    metrics: Mutex<BTreeMap<String, f64>>,
}

#[tokio::main]
async fn main() {
    let addr: SocketAddr = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(SocketAddr::new(
            std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
            8429,
        ));

    let state = Arc::new(State {
        metrics: Mutex::new(BTreeMap::new()),
    });
    let app = Router::new().with_state(state);

    let listen = TcpListener::bind(addr).await.unwrap();
    println!("listening on {}", listen.local_addr().unwrap());
    axum::serve(listen, app.into_make_service()).await.unwrap();
}
