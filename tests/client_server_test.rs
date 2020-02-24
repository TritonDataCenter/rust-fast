// Copyright 2020 Joyent, Inc.

use std::io::{Error, ErrorKind};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::process;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use serde_json::Value;
use slog::{debug, error, info, o, Drain, Logger};
use tokio::net::TcpListener;
use tokio::prelude::*;

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

fn run_server(barrier: Arc<Barrier>) {
    let plain = slog_term::PlainSyncDecorator::new(std::io::stdout());
    let root_log = Logger::root(
        Mutex::new(slog_term::FullFormat::new(plain).build()).fuse(),
        o!("build-id" => "0.1.0"),
    );

    let addr_str = "127.0.0.1:56652".to_string();
    match addr_str.parse::<SocketAddr>() {
        Ok(addr) => {
            let listener = TcpListener::bind(&addr).expect("failed to bind");
            info!(root_log, "listening for fast requests"; "address" => addr);

            barrier.wait();

            tokio::run({
                let process_log = root_log.clone();
                let err_log = root_log.clone();
                listener
                    .incoming()
                    .map_err(move |e| error!(&err_log, "failed to accept socket"; "err" => %e))
                    .for_each(move |socket| {
                        let task = server::make_task(socket, msg_handler, Some(&process_log));
                        tokio::spawn(task);
                        Ok(())
                    })
            })
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

#[test]
fn client_server_comms() {
    let barrier = Arc::new(Barrier::new(2));
    let barrier_clone = barrier.clone();
    let _h_server = thread::spawn(move || run_server(barrier_clone));

    barrier.clone().wait();

    let addr_str = "127.0.0.1:56652".to_string();
    let addr = addr_str.parse::<SocketAddr>().unwrap();

    let mut stream = TcpStream::connect(&addr).unwrap_or_else(|e| {
        eprintln!("Failed to connect to server: {}", e);
        process::exit(1)
    });

    (1..100).for_each(|x| {
        let data_size = x * 1000;
        let method = String::from("echo");
        let args_str = ["[\"", &"a".repeat(data_size), "\"]"].concat();
        let args: Value = serde_json::from_str(&args_str).unwrap();
        let handler = response_handler(data_size);
        let mut msg_id = FastMessageId::new();
        let result = client::send(method, args, &mut msg_id, &mut stream)
            .and_then(|_bytes_written| client::receive(&mut stream, handler));

        assert!(result.is_ok());
    });

    let shutdown_result = stream.shutdown(Shutdown::Both);

    assert!(shutdown_result.is_ok());
}
