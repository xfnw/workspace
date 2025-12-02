// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    hash::Hasher,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{
    net::TcpListener,
    sync::{RwLock, mpsc},
    task::AbortHandle,
};

#[derive(Debug)]
struct Client {
    nick: String,
    sender: mpsc::Sender<Vec<u8>>,
}

#[derive(Debug)]
struct Job {
    callback: mpsc::Sender<u64>,
    handle: AbortHandle,
}

#[derive(Debug)]
struct AppState {
    clients: RwLock<Vec<Option<Client>>>,
    active: RwLock<BTreeSet<usize>>,
    job: RwLock<Option<Job>>,
    job_sent: AtomicUsize,
    job_total: AtomicUsize,
}

#[derive(Debug, Serialize)]
struct StatusClient {
    nick: String,
    active: bool,
}

#[derive(Debug, Serialize)]
struct StatusReply {
    clients: Vec<Option<StatusClient>>,
    job_sent: usize,
    job_total: usize,
}

async fn status(State(state): State<Arc<AppState>>) -> Json<StatusReply> {
    let clients = state.clients.read().await;
    let active = state.active.read().await;
    let job_sent = state.job_sent.load(Ordering::SeqCst);
    let job_total = state.job_total.load(Ordering::SeqCst);
    Json(StatusReply {
        clients: clients
            .iter()
            .enumerate()
            .map(|(n, c)| {
                c.as_ref().map(|c| StatusClient {
                    nick: c.nick.clone(),
                    active: active.contains(&n),
                })
            })
            .collect(),
        job_sent,
        job_total,
    })
}

fn hash_line(nick: &[u8], command: &[u8], trail: &[u8]) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    hasher.write(nick);
    hasher.write(b" ");
    hasher.write(command);
    hasher.write(b" ");
    hasher.write(trail);
    hasher.finish()
}

#[tokio::main]
async fn main() {
    let addr: SocketAddr = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(SocketAddr::new(
            std::net::IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
            8667,
        ));

    let state = Arc::new(AppState {
        clients: RwLock::new(vec![]),
        active: RwLock::new(BTreeSet::new()),
        job: RwLock::new(Option::None),
        job_sent: AtomicUsize::new(0),
        job_total: AtomicUsize::new(0),
    });
    let app = Router::new()
        .route("/status", get(status))
        .with_state(state);

    let listen = TcpListener::bind(addr).await.unwrap();
    eprintln!("listening on {}", listen.local_addr().unwrap());
    axum::serve(listen, app.into_make_service()).await.unwrap();
}
