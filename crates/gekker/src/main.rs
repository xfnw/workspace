// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
};
use irc_connect::tokio_rustls::rustls::{
    RootCertStore,
    pki_types::{CertificateDer, pem::PemObject},
};
use serde::{Deserialize, Serialize};
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
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
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
    autojoin: RwLock<Option<String>>,
    job: RwLock<Job>,
    job_sent: AtomicUsize,
    job_total: AtomicUsize,
    ca_certs: Arc<RootCertStore>,
}

#[derive(Debug, Serialize)]
struct StatusClient {
    nick: String,
    active: bool,
}

#[derive(Debug, Serialize)]
struct StatusReply {
    clients: Vec<Option<StatusClient>>,
    autojoin: Option<String>,
    job_active: bool,
    job_sent: usize,
    job_total: usize,
}

async fn status(State(state): State<Arc<AppState>>) -> Json<StatusReply> {
    let clients = state.clients.read().await;
    let active = state.active.read().await;
    let autojoin = state.autojoin.read().await.clone();
    let job_active = !state.job.read().await.handle.is_finished();
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
        autojoin,
        job_active,
        job_sent,
        job_total,
    })
}

fn hash_line(nick: &[u8], command: &str, trail: &[u8]) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    hasher.write(nick);
    hasher.write(b" ");
    hasher.write(command.as_bytes());
    hasher.write(b" ");
    hasher.write(trail);
    hasher.finish()
}

#[derive(Debug, Deserialize)]
struct ConnectArgs {
    nick: String,
    host: String,
    socks5: Option<SocketAddr>,
    #[serde(default)]
    plaintext: bool,
    #[serde(default)]
    insecure: bool,
}

async fn connect(
    State(state): State<Arc<AppState>>,
    Query(args): Query<ConnectArgs>,
) -> Result<(), (StatusCode, String)> {
    let conn = irc_connect::Stream::new_tcp(args.host);
    let conn = if let Some(addr) = args.socks5 {
        conn.socks5(addr)
    } else {
        conn
    };
    let conn = if args.plaintext {
        conn
    } else if args.insecure {
        conn.tls_danger_insecure(None)
    } else {
        conn.tls_with_root(None, state.ca_certs.clone())
    };
    let mut conn = conn
        .connect()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let (slot, receiver) = reserve_client_slot(&state.clients).await;
    conn.write_all(format!("NICK {}\r\nUSER {0} 0 * {0}\r\n", args.nick).as_bytes())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tokio::spawn(async move {
        eprintln!("{slot} connected!");
        client_loop(state.clone(), slot, conn, receiver).await;
        eprintln!("{slot} disconnected!");
        state.clients.write().await[slot] = None;
    });
    Ok(())
}

async fn reserve_client_slot(
    clients: &RwLock<Vec<Option<Client>>>,
) -> (usize, mpsc::Receiver<Vec<u8>>) {
    let (sender, receiver) = mpsc::channel(6);
    let client = Client {
        nick: "???".to_string(),
        sender,
    };
    let mut clients = clients.write().await;
    let slot = clients.iter().position(Option::is_none).unwrap_or_else(|| {
        let len = clients.len();
        clients.push(None);
        len
    });
    assert!(clients[slot].is_none());
    clients[slot] = Some(client);
    (slot, receiver)
}

async fn client_loop(
    state: Arc<AppState>,
    slot: usize,
    conn: irc_connect::Stream,
    mut receiver: mpsc::Receiver<Vec<u8>>,
) {
    let (read, mut write) = tokio::io::split(conn);
    let mut read = BufReader::new(read);
    let mut ircbuf = Vec::with_capacity(512);
    loop {
        tokio::select! {
            Ok(len) = read.read_until(b'\n', &mut ircbuf) => {
                if len == 0 {
                    return;
                }
                while ircbuf.pop_if(|c| b"\r\n".contains(c)).is_some() {}
                let Ok(mut line) = irctokens::Line::tokenise(&ircbuf) else {
                    return;
                };
                ircbuf.clear();
                line.command.make_ascii_uppercase();
                let source_nick = line.source.as_ref().and_then(|s| s.split(|&b| b == b'!').next());
                if let Some(nick) = source_nick
                    && let Some(trailing) = line.arguments.last()
                {
                    let h = hash_line(nick, &line.command, trailing);
                    _ = state.job.read().await.callback.send(h).await;
                }
                match line.command.as_ref() {
                    "PING" => {
                        line.source = None;
                        line.command = "PONG".to_string();
                        let mut out = line.format();
                        out.extend_from_slice(b"\r\n");
                        if write.write_all(&out).await.is_err() {
                            return;
                        }
                        continue;
                    }
                    "001" => {
                        if let Some(mynick) = line.arguments.first().and_then(|n| str::from_utf8(n).ok()) {
                            let mut clients = state.clients.write().await;
                            clients[slot].as_mut().unwrap().nick = mynick.to_string();
                        }
                        if let Some(channel) = state.autojoin.read().await.as_ref() {
                            let out = irctokens::Line {
                                tags: None,
                                source: None,
                                command: "JOIN".to_string(),
                                arguments: vec![channel.as_bytes().to_vec()],
                            };
                            let mut out = out.format();
                            out.extend_from_slice(b"\r\n");
                            if write.write_all(&out).await.is_err() {
                                return;
                            }
                        }
                    }
                    "366" => {
                        state.active.write().await.insert(slot);
                    }
                    _ => (),
                }
            }
            Some(mut line) = receiver.recv() => {
                line.extend_from_slice(b"\r\n");
                if write.write_all(&line).await.is_err() {
                    return;
                }
            }
            else => {
                return;
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct MaybeChannel {
    channel: Option<String>,
}

async fn set_autojoin(
    State(state): State<Arc<AppState>>,
    Query(MaybeChannel { channel }): Query<MaybeChannel>,
) {
    *state.autojoin.write().await = channel;
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

    let mut ca_certs = RootCertStore::empty();
    ca_certs.add_parsable_certificates(
        CertificateDer::pem_file_iter("/etc/ssl/certs/ca-bundle.crt")
            .unwrap()
            .flatten(),
    );
    let fake_job = Job {
        callback: mpsc::channel(1).0,
        handle: tokio::spawn(async {}).abort_handle(),
    };

    let state = Arc::new(AppState {
        clients: RwLock::new(vec![]),
        active: RwLock::new(BTreeSet::new()),
        autojoin: RwLock::new(None),
        job: RwLock::new(fake_job),
        job_sent: AtomicUsize::new(0),
        job_total: AtomicUsize::new(0),
        ca_certs: Arc::new(ca_certs),
    });
    let app = Router::new()
        .route("/status", get(status))
        .route("/autojoin", post(set_autojoin))
        .route("/connect", post(connect))
        .with_state(state);

    let listen = TcpListener::bind(addr).await.unwrap();
    eprintln!("listening on {}", listen.local_addr().unwrap());
    axum::serve(listen, app.into_make_service()).await.unwrap();
}
