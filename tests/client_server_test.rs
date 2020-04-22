// Copyright 2020 Joyent, Inc.

use std::error::Error as StdError;
use std::io::{Error, ErrorKind};
use std::net::{Shutdown, SocketAddr};
use std::process;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use futures::StreamExt;
use serde_json::Value;
use slog::{debug, info, o, Drain, Level, LevelFilter, Logger};
use tokio::net::{TcpListener, TcpStream};
use tokio_test::block_on;

use fast_rpc::client;
use fast_rpc::protocol::{FastMessage, FastMessageId};
use fast_rpc::server;

fn echo_handler(
    msg: &FastMessage,
    mut response: Vec<FastMessage>,
    log: &Logger,
) -> Result<Vec<FastMessage>, Error> {
    debug!(log, "handling echo function request");
    response.push(FastMessage::data(msg.id, msg.data.clone()));
    Ok(response)
}

fn msg_handler(
    msg: &FastMessage,
    log: &Logger,
) -> Result<Vec<FastMessage>, Error> {
    let response: Vec<FastMessage> = vec![];

    match msg.data.m.name.as_str() {
        "echo" => echo_handler(msg, response, &log),
        _ => Err(Error::new(
            ErrorKind::Other,
            format!("Unsupported function: {}", msg.data.m.name),
        )),
    }
}

#[tokio::main]
async fn run_server(barrier: Arc<Barrier>) {
    let plain = slog_term::PlainSyncDecorator::new(std::io::stdout());
    let root_log = Logger::root(
        Mutex::new(LevelFilter::new(
            slog_term::FullFormat::new(plain).build(),
            Level::Info,
        ))
        .fuse(),
        o!("build-id" => "0.1.0"),
    );

    let addr_str = "127.0.0.1:56652".to_string();
    match addr_str.parse::<SocketAddr>() {
        Ok(addr) => {
            let mut listener =
                TcpListener::bind(&addr).await.expect("failed to bind");
            let mut incoming = listener.incoming();
            info!(root_log, "listening for fast requests"; "address" => addr);

            barrier.wait();

            while let Some(Ok(stream)) = incoming.next().await {
                let process_log = root_log.clone();
                tokio::spawn(async move {
                    server::make_task(stream, msg_handler, Some(&process_log))
                        .await;
                });
            }

            ()
        }
        Err(e) => {
            eprintln!("error parsing address: {}", e);
        }
    }
}

fn assert_handler(expected_data_size: usize) -> impl Fn(&FastMessage) {
    move |msg| {
        let data: Vec<String> =
            serde_json::from_value(msg.data.d.clone()).unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].len(), expected_data_size);
    }
}

fn response_handler(
    data_size: usize,
) -> impl Fn(&FastMessage) -> Result<(), Error> {
    let handler = assert_handler(data_size);
    move |msg| {
        handler(msg);
        Ok(())
    }
}

async fn run_client() -> Result<(), Box<dyn StdError>> {
    let addr_str = "127.0.0.1:56652".to_string();
    let addr = addr_str.parse::<SocketAddr>().unwrap();

    let mut stream = TcpStream::connect(&addr).await.unwrap_or_else(|e| {
        eprintln!("Failed to connect to server: {}", e);
        process::exit(1)
    });

    for i in 1..100 {
        let data_size = i * 1000;
        let method = String::from("echo");
        let args_str = ["[\"", &"a".repeat(data_size), "\"]"].concat();
        let args: Value = serde_json::from_str(&args_str).unwrap();
        let handler = response_handler(data_size);
        let mut msg_id = FastMessageId::new();
        client::send(method, args, &mut msg_id, &mut stream).await?;
        let result = client::receive(&mut stream, handler).await;

        assert!(result.is_ok());
    }

    let shutdown_result = stream.shutdown(Shutdown::Both);

    assert!(shutdown_result.is_ok());

    Ok(())
}

#[test]
fn client_server_comms() {
    let barrier = Arc::new(Barrier::new(2));
    let barrier_clone = barrier.clone();
    let _h_server = thread::spawn(move || run_server(barrier_clone));

    barrier.clone().wait();
    assert!(block_on(run_client()).is_ok());
}
