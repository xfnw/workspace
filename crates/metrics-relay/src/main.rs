// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

use axum::{Router, extract::State, routing::get};
use std::{
    collections::BTreeMap,
    fmt::Write,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::net::TcpListener;

mod influx;
mod prom;

struct AppState {
    metrics: Mutex<BTreeMap<String, f64>>,
}

async fn get_metrics(State(state): State<Arc<AppState>>) -> String {
    let mut out = String::new();
    for (k, v) in state.metrics.lock().unwrap().iter() {
        writeln!(out, "{k} {v}").unwrap();
    }
    out
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

    let state = Arc::new(AppState {
        metrics: Mutex::new(BTreeMap::new()),
    });
    let app = Router::new()
        .route("/metrics", get(get_metrics))
        .with_state(state);

    let listen = TcpListener::bind(addr).await.unwrap();
    println!("listening on {}", listen.local_addr().unwrap());
    axum::serve(listen, app.into_make_service()).await.unwrap();
}
