use std::env;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, TcpStream};
use std::process;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use serde_json::Value;
use slog::{debug, error, info, o, Drain, Logger};
use tokio::net::TcpListener;
use tokio::prelude::*;

use rust_fast::client;
use rust_fast::protocol::FastMessage;
use rust_fast::server;

fn echo_handler(
    msg: &FastMessage,
    mut response: Vec<FastMessage>,
    log: &Logger,
) -> Result<Vec<FastMessage>, Error> {
    debug!(log, "handling echo function request");
    response.push(FastMessage::data(msg.id, msg.data.clone()));
    Ok(response)
}

fn msg_handler(msg: &FastMessage, log: &Logger) -> Result<Vec<FastMessage>, Error> {
    let response: Vec<FastMessage> = vec![];

    match msg.data.m.name.as_str() {
        "echo" => echo_handler(msg, response, &log),
        _ => Err(Error::new(
            ErrorKind::Other,
            format!("Unsupport functon: {}", msg.data.m.name),
        )),
    }
}

fn run_server(barrier: Arc<Barrier>) {
    let plain = slog_term::PlainSyncDecorator::new(std::io::stdout());
    let root_log = Logger::root(
        Mutex::new(slog_term::FullFormat::new(plain).build()).fuse(),
        o!("build-id" => "0.1.0"),
    );

    let addr_str = env::args().nth(1).unwrap_or("127.0.0.1:56655".to_string());
    let addr = addr_str.parse::<SocketAddr>().unwrap();

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
                let task = server::make_task(socket, msg_handler, &process_log);
                tokio::spawn(task);
                Ok(())
            })
    });
}

fn assert_handler(msg: &FastMessage) {
    println!("{}", msg.data.d);
    let args_str = "[\"abc\"]";
    let args: Value = serde_json::from_str(args_str).unwrap();
    assert_eq!(msg.data.d, args);
}

fn response_handler(msg: &FastMessage) -> Result<(), Error> {
    match msg.data.m.name.as_str() {
        "date" | "echo" | "yes" | "getobject" | "putobject" => assert_handler(msg),
        _ => println!("Received {} response", msg.data.m.name),
    }

    Ok(())
}

#[test]
fn client_server_comms() {
    let barrier = Arc::new(Barrier::new(2));
    let barrier_clone = barrier.clone();
    let _h_server = thread::spawn(move || run_server(barrier_clone));

    barrier.clone().wait();

    let addr_str = env::args().nth(1).unwrap_or("127.0.0.1:56655".to_string());
    let addr = addr_str.parse::<SocketAddr>().unwrap();

    let mut stream = TcpStream::connect(&addr).unwrap_or_else(|e| {
        eprintln!("Failed to connect to server: {}", e);
        process::exit(1)
    });

    let method = String::from("echo");
    let args_str = "[\"abc\"]";
    let args: Value = serde_json::from_str(args_str).unwrap();
    let _result = client::send(method, args, &mut stream)
        .and_then(|_bytes_written| client::receive(&mut stream, response_handler));

    assert!(true);
}
