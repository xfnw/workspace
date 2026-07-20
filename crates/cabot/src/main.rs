// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

use argh::{FromArgs, from_env};
use irc_connect::{
    Connection,
    tokio_rustls::rustls::{
        RootCertStore,
        pki_types::{CertificateDer, pem::PemObject},
    },
};
use std::{path::PathBuf, sync::Arc};
use tokio::{
    io::{self, AsyncWriteExt},
    sync::broadcast,
};

mod bot;
mod file_store;

/// content addressed irc bot
#[derive(Debug, FromArgs)]
struct Opt {
    /// number of lines to cache, defaults to 10000
    #[argh(option, short = 'c', default = "10000")]
    capacity: usize,
    /// number of milliseconds to mostly wait between sending messages
    #[argh(option, short = 'd', default = "0")]
    delay: u64,
    /// do not announce added content until actually requested
    #[argh(switch)]
    stealthy: bool,
    /// mountpoint to mount fuse filesystem
    #[argh(option, arg_name = "mountpoint")]
    fuse: Option<PathBuf>,
    /// hash to resume fuse filesystem state from
    #[argh(option, arg_name = "digest", from_str_fn(parse_hex_digest))]
    fuse_resume: Option<[u8; 16]>,
    /// seconds between automatically syncing fuse filesystem
    #[argh(option, arg_name = "interval", default = "9")]
    fuse_interval: u64,
    /// seconds before giving up on realizing
    #[argh(option, arg_name = "timeout", default = "10")]
    fuse_timeout: u64,
    /// nickname to use
    #[argh(option, short = 'n', default = "\"ca\".to_string()")]
    nick: String,
    /// channel to automatically join
    #[argh(option, short = 'j', default = "\"#ca\".to_string()")]
    join: String,
    /// enable tls
    #[argh(switch, short = 't')]
    tls: bool,
    /// path of certificate store to trust
    #[argh(
        option,
        short = 'T',
        default = "\"/etc/ssl/certs/ca-bundle.crt\".into()"
    )]
    trust: PathBuf,
    /// irc server address to connect to
    #[argh(positional)]
    addr: String,
}

#[derive(Debug, foxerror::FoxError)]
pub enum Error {
    #[err(from)]
    Io(io::Error),
    Broadcast(broadcast::error::RecvError),
    FileInvalidHashes,
    FileTooManyHashes,
    #[err(from)]
    Base64Decode(base64::DecodeError),
    ParseDirectory,
    #[err(from)]
    Timeout(tokio::time::error::Elapsed),
    Poisoned,
    Replaced,
}

fn parse_hex_digest(inp: &str) -> Result<[u8; 16], String> {
    const_hex_lite::unhex_array(inp.as_bytes()).ok_or_else(|| "invalid digest".to_string())
}

#[tokio::main]
async fn main() {
    let opt: Opt = from_env();

    let stream = Connection::new_tcp(opt.addr);
    let stream = if opt.tls {
        let mut root = RootCertStore::empty();
        root.add_parsable_certificates(CertificateDer::pem_file_iter(opt.trust).unwrap().flatten());
        stream.tls_with_root(None, root)
    } else {
        stream
    };
    let mut stream = stream.connect().await.unwrap();

    stream
        .write_all(
            format!(
                "NICK {}\r\nUSER ca 0 * :content addressed bot\r\n",
                opt.nick
            )
            .as_bytes(),
        )
        .await
        .unwrap();

    let bot = Arc::new(bot::Bot::new(
        stream,
        opt.join,
        opt.delay,
        opt.capacity,
        opt.stealthy,
    ));

    let bot_handle = {
        let bot = bot.clone();
        tokio::spawn(async move { bot.run().await })
    };

    if let Some(mountpoint) = opt.fuse {
        let file_store = file_store::IrcFileStore::new(bot.clone());
        let filesystem = cabotfs::CaFilesystem::new(file_store, opt.fuse_resume, opt.fuse_timeout);
        let mount_handle = cabotfs::mount(filesystem.clone(), &mountpoint)
            .await
            .unwrap();

        let filesystem_ = filesystem.clone();
        tokio::spawn(async move {
            let res = bot_handle.await;
            filesystem_.poison();
            res.unwrap().unwrap();
        });

        tokio::spawn(async move {
            let duration = std::time::Duration::from_secs(opt.fuse_interval);
            loop {
                tokio::time::sleep(duration).await;
                if filesystem.sync().await.is_err() {
                    break;
                }
            }
        });

        mount_handle.await.unwrap();

        return;
    }

    bot_handle.await.unwrap().unwrap();
}
