// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
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
    nick: RwLock<String>,
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
    let active = state.active.read().await;
    let mut clients = Vec::new();
    for (n, client) in state.clients.read().await.iter().enumerate() {
        clients.push(if let Some(client) = client {
            Some(StatusClient {
                nick: client.nick.read().await.clone(),
                active: active.contains(&n),
            })
        } else {
            None
        });
    }
    let autojoin = state.autojoin.read().await.clone();
    let job_active = !state.job.read().await.handle.is_finished();
    let job_sent = state.job_sent.load(Ordering::SeqCst);
    let job_total = state.job_total.load(Ordering::SeqCst);
    Json(StatusReply {
        clients,
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
    hasher.write(trail.trim_ascii());
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
        nick: RwLock::new("???".to_string()),
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
                        ircbuf.clear();
                        continue;
                    }
                    "PRIVMSG" | "NOTICE" => {
                        ircbuf.clear();
                        continue;
                    }
                    "NICK" => {
                        if let Some(oldnick) = source_nick.and_then(|n| str::from_utf8(n).ok())
                            && oldnick == *state.clients.read().await[slot].as_ref().unwrap().nick.read().await
                            && let Some(newnick) = line.arguments.first().and_then(|n| str::from_utf8(n).ok())
                        {
                            let clients = state.clients.read().await;
                            *clients[slot].as_ref().unwrap().nick.write().await = newnick.to_string();
                        }
                    }
                    "001" => {
                        if let Some(mynick) = line.arguments.first().and_then(|n| str::from_utf8(n).ok()) {
                            let clients = state.clients.read().await;
                            *clients[slot].as_ref().unwrap().nick.write().await = mynick.to_string();
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

                println!("{slot}> {}", String::from_utf8_lossy(&ircbuf));
                ircbuf.clear();
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

async fn dispatch_job<F>(
    state: Arc<AppState>,
    body: Bytes,
    callback: impl FnOnce(Arc<AppState>, Vec<Vec<u8>>) -> F + Send + 'static,
) -> Result<(), (StatusCode, &'static str)>
where
    F: Future<Output = ()> + Send + 'static,
{
    let lines: Vec<Vec<u8>> = body
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .map(<[u8]>::to_vec)
        .collect();
    let mut job = state.job.write().await;
    if !job.handle.is_finished() {
        return Err((
            StatusCode::CONFLICT,
            "there is already a job running. cancel it to start a new one",
        ));
    }
    state.job_sent.store(0, Ordering::SeqCst);
    state.job_total.store(lines.len(), Ordering::SeqCst);

    let state = state.clone();
    let task = tokio::spawn(async move { callback(state, lines).await });
    job.handle = task.abort_handle();

    Ok(())
}

async fn raw_all(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<(), (StatusCode, &'static str)> {
    dispatch_job(state, body, async |state, lines| {
        for line in lines {
            for client in state.clients.read().await.iter().flatten() {
                _ = client.sender.try_send(line.clone());
            }
            state.job_sent.fetch_add(1, Ordering::SeqCst);
        }
    })
    .await
}

async fn raw_slot(
    State(state): State<Arc<AppState>>,
    Path(slot): Path<usize>,
    body: Bytes,
) -> Result<(), (StatusCode, &'static str)> {
    dispatch_job(state, body, async move |state, lines| {
        for line in lines {
            let clients = state.clients.read().await;
            let Some(Some(client)) = clients.get(slot) else {
                return;
            };
            if client.sender.try_send(line).is_err() {
                return;
            }
            state.job_sent.fetch_add(1, Ordering::SeqCst);
        }
    })
    .await
}

#[derive(Debug, Deserialize)]
struct SendOpt {
    command: Option<String>,
    arg: Option<String>,
}

async fn send(
    State(state): State<Arc<AppState>>,
    Query(opt): Query<SendOpt>,
    body: Bytes,
) -> Result<(), (StatusCode, &'static str)> {
    dispatch_job(state, body, async move |state, lines| {
        let mut lines = lines.into_iter();
        let mut command = opt.command.unwrap_or_else(|| "PRIVMSG".to_string());
        command.make_ascii_uppercase();
        let command = command;
        let args: Vec<Vec<u8>> = opt.arg.into_iter().map(String::into_bytes).collect();
        let mut hashes = Vec::with_capacity(128);
        let (sender, mut receiver) = mpsc::channel(128);
        state.job.write().await.callback = sender;

        loop {
            let active = state.active.read().await.clone();
            if active.is_empty() {
                return;
            }
            for &slot in &active {
                let hash = {
                    let clients = state.clients.read().await;
                    let Some(client) = &clients[slot] else {
                        state.active.write().await.remove(&slot);
                        continue;
                    };
                    let nick = &client.nick.read().await;
                    let Some(trail) = lines.next() else {
                        return;
                    };
                    let hash = hash_line(nick.as_bytes(), &command, &trail);
                    let mut line = irctokens::Line {
                        tags: None,
                        source: None,
                        command: command.clone(),
                        arguments: args.clone(),
                    };
                    line.arguments.push(trail);
                    _ = client.sender.try_send(line.format());
                    hash
                };

                if tokio::time::timeout(std::time::Duration::from_secs(1), async {
                    while !hashes.contains(&hash) {
                        hashes.clear();
                        receiver.recv_many(&mut hashes, 128).await;
                    }
                })
                .await
                .is_err()
                    && active.len() > 1
                {
                    state.active.write().await.remove(&slot);
                }

                state.job_sent.fetch_add(1, Ordering::SeqCst);
            }
        }
    })
    .await
}

async fn cancel(State(state): State<Arc<AppState>>) {
    state.job.read().await.handle.abort();
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
        .route("/raw/all", post(raw_all))
        .route("/raw/{slot}", post(raw_slot))
        .route("/send", post(send))
        .route("/cancel", post(cancel))
        .with_state(state);

    let listen = TcpListener::bind(addr).await.unwrap();
    eprintln!("listening on {}", listen.local_addr().unwrap());
    axum::serve(listen, app.into_make_service()).await.unwrap();
}
