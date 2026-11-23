// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: EUPL-1.2

use std::{
    borrow::Cow,
    error::Error,
    io::ErrorKind,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{body::BufReadExactBody, token::TokenHandler};
use append_kv::AppendKV;
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::{
    Method, Request, Response, StatusCode,
    body::{Body, Bytes, Incoming},
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use percent_encoding::percent_decode_str;
use tokio::{
    fs::File,
    io::{AsyncSeekExt, AsyncWriteExt, BufReader, BufWriter},
    net::TcpListener,
};

/// run the http server
#[derive(Debug, argh::FromArgs)]
#[argh(subcommand, name = "serve")]
#[argh(help_triggers("-h", "--help"))]
pub struct Args {
    /// address to listen on
    #[argh(option, arg_name = "addr", default = "\"[::]:8082\".parse().unwrap()")]
    bind: SocketAddr,
    /// require auth token to PUT, signed by the key in file
    #[argh(option, arg_name = "file")]
    key_file: Option<PathBuf>,
    /// max upload size in MiB (default 1024)
    #[argh(option, default = "1024")]
    limit: u64,
    /// allow PUTs for paths that already exist
    #[argh(switch)]
    shadow: bool,
    #[argh(positional)]
    database: PathBuf,
}

struct Server {
    db: AppendKV,
    db_path: PathBuf,
    token_handler: Option<TokenHandler>,
    limit: u64,
    shadow: bool,
}

impl Server {
    async fn new(
        db_path: PathBuf,
        key_file: Option<&Path>,
        limit: u64,
        shadow: bool,
    ) -> std::io::Result<Self> {
        let db = AppendKV::open(&mut BufReader::new(
            File::options()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&db_path)
                .await?,
        ))
        .await?;
        let token_handler = if let Some(key_file) = key_file {
            let key = tokio::fs::read(key_file).await?;
            Some(TokenHandler::new(&key))
        } else {
            None
        };
        Ok(Self {
            db,
            db_path,
            token_handler,
            limit,
            shadow,
        })
    }

    async fn handle(
        self: Arc<Self>,
        req: Request<Incoming>,
    ) -> Result<Response<BoxBody<Bytes, std::io::Error>>, hyper::http::Error> {
        match req.method() {
            &Method::GET | &Method::HEAD => self.handle_get(req).await,
            &Method::PUT => self
                .handle_put(req)
                .await
                .map(|r| r.map(|b| b.map_err(|e| match e {}).boxed())),
            method => Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .header("content-type", "text/plain; charset=UTF-8")
                .body(
                    Full::new(Bytes::from_owner(format!(
                        "method {method} is not supported\n"
                    )))
                    .map_err(|e| match e {})
                    .boxed(),
                ),
        }
    }

    async fn handle_get(
        &self,
        req: Request<Incoming>,
    ) -> Result<Response<BoxBody<Bytes, std::io::Error>>, hyper::http::Error> {
        let path = Cow::from(percent_decode_str(req.uri().path()));
        let path = path.strip_prefix(b"/").unwrap_or(&path);

        let (mut file, len) = match self.get_file(path).await {
            Ok(f) => f,
            Err(e) => {
                return Response::builder()
                    .status(
                        if e.downcast_ref::<std::io::Error>()
                            .is_some_and(|e| e.kind() == ErrorKind::NotFound)
                        {
                            StatusCode::NOT_FOUND
                        } else {
                            StatusCode::INTERNAL_SERVER_ERROR
                        },
                    )
                    .header("content-type", "text/plain; charset=UTF-8")
                    .body(
                        Full::new(Bytes::from_owner(format!("oh noes: {e}\n")))
                            .map_err(|e| match e {})
                            .boxed(),
                    );
            }
        };

        let guess;
        let res = Response::builder().header(
            "content-type",
            if path.ends_with(b"/") {
                "text/html"
            } else {
                guess = mime_guess::from_ext(
                    str::from_utf8(
                        &path[path.iter().rposition(|&b| b == b'.').map_or(0, |p| p + 1)..],
                    )
                    .unwrap_or(""),
                )
                .first_or_text_plain();
                guess.essence_str()
            },
        );

        let (res, len) = if let Some(range) = req.headers().get("range") {
            let Some((range, start)) = crate::ranges::parse(range, len)
                .and_then(|r| i64::try_from(*r.start()).ok().map(|s| (r, s)))
            else {
                return Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header("content-range", format!("bytes */{len}"))
                    .body(
                        Full::new(Bytes::from_static(b"bad range\n"))
                            .map_err(|e| match e {})
                            .boxed(),
                    );
            };

            if let Err(e) = file.seek(std::io::SeekFrom::Current(start)).await {
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("content-type", "text/plain; charset=UTF-8")
                    .body(
                        Full::new(Bytes::from_owner(format!("oh noes: {e}\n")))
                            .map_err(|e| match e {})
                            .boxed(),
                    );
            }

            (
                res.status(StatusCode::PARTIAL_CONTENT).header(
                    "content-range",
                    format!("bytes {}-{}/{len}", range.start(), range.end()),
                ),
                range.end() - range.start() + 1,
            )
        } else {
            (res, len)
        };

        let body = BufReadExactBody::new(file, len).boxed();

        res.header("accept-ranges", "bytes")
            .header("access-control-allow-origin", "*")
            .body(body)
    }

    async fn get_file(&self, path: &[u8]) -> Result<(BufReader<File>, u64), Box<dyn Error>> {
        let mut db_file = BufReader::new(File::options().read(true).open(&self.db_path).await?);
        let len = self.db.get(&mut db_file, path).await?;

        Ok((db_file, len))
    }

    async fn handle_put(
        &self,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, hyper::http::Error> {
        if let Some(handler) = &self.token_handler
            && !req
                .headers()
                .get("authorization")
                .is_some_and(|h| handler.verify_header(h))
        {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("content-type", "text/plain; charset=UTF-8")
                .body(Full::new(Bytes::from_static(b"unauthorized\n")));
        }

        if let Err(e) = self.upload(req).await {
            return Response::builder()
                .status(
                    if e.downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == ErrorKind::FileTooLarge)
                    {
                        StatusCode::PAYLOAD_TOO_LARGE
                    } else {
                        StatusCode::INTERNAL_SERVER_ERROR
                    },
                )
                .header("content-type", "text/plain; charset=UTF-8")
                .body(Full::new(Bytes::from_owner(format!(
                    "failed to upload: {e}\n"
                ))));
        }

        Response::builder()
            .status(StatusCode::CREATED)
            .header("content-type", "text/plain; charset=UTF-8")
            .body(Full::new(Bytes::from_static(b"uploaded~\n")))
    }

    async fn upload(&self, req: Request<Incoming>) -> Result<(), Box<dyn Error>> {
        let (parts, mut body) = req.into_parts();
        if body.size_hint().lower() > self.limit {
            return Err(std::io::Error::from(ErrorKind::FileTooLarge).into());
        }

        let path = Cow::from(percent_decode_str(parts.uri.path()));
        let path = path.strip_prefix(b"/").unwrap_or(&path);

        let temp = tokio::task::spawn_blocking(tempfile::tempfile).await??;
        let mut temp = File::from_std(temp);
        let mut size = 0;

        while let Some(frame) = body.frame().await {
            let frame = frame?;
            let Ok(data) = frame.into_data() else {
                continue;
            };

            size += data.len() as u64;

            if size > self.limit {
                return Err(std::io::Error::from(ErrorKind::FileTooLarge).into());
            }

            temp.write_all(&data).await?;
        }

        let mut db_file = BufWriter::new(File::options().write(true).open(&self.db_path).await?);
        if self.shadow {
            self.db.insert(&mut db_file, path.to_vec(), size).await?;
        } else {
            self.db
                .try_insert(&mut db_file, path.to_vec(), size)
                .await?;
        }
        temp.rewind().await?;
        tokio::io::copy(&mut temp, &mut db_file).await?;
        db_file.flush().await?;
        Ok(())
    }
}

pub async fn serve(args: Args) -> Result<(), Box<dyn Error>> {
    let server = Arc::new(
        Server::new(
            args.database,
            args.key_file.as_deref(),
            args.limit * (1 << 20),
            args.shadow,
        )
        .await?,
    );
    let listen = TcpListener::bind(args.bind).await?;
    println!("listening on {}", listen.local_addr()?);

    loop {
        let (stream, _) = listen.accept().await?;
        let io = TokioIo::new(stream);
        let server = server.clone();
        let service = service_fn(move |req| server.clone().handle(req));
        tokio::spawn(async move {
            _ = http1::Builder::new().serve_connection(io, service).await;
        });
    }
}
