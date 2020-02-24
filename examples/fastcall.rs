// Copyright 2020 Joyent, Inc.

use std::io::Error;
use std::net::{SocketAddr, TcpStream};
use std::process;

use clap::{crate_version, value_t, App, Arg, ArgMatches};
use serde_json::Value;

use fast_rpc::client;
use fast_rpc::protocol::{FastMessage, FastMessageId};

static APP: &'static str = "fastcall";
static DEFAULT_HOST: &'static str = "127.0.0.1";
const DEFAULT_PORT: u32 = 2030;

pub fn parse_opts<'a, 'b>(app: String) -> ArgMatches<'a> {
    App::new(app)
        .about("Command-line tool for making a node-fast RPC method call")
        .version(crate_version!())
        .arg(
            Arg::with_name("host")
                .help("DNS name or IP address for remote server")
                .long("host")
                .short("h")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("port")
                .help("TCP port for remote server (Default: 2030)")
                .long("port")
                .short("p")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("method")
                .help("Name of remote RPC method call")
                .long("method")
                .short("m")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("args")
                .help("JSON-encoded arguments for RPC method call")
                .long("args")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("abandon")
                .long("abandon-immediately")
                .short("a")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("leave_open")
                .long("leave-conn-open")
                .short("c")
                .takes_value(false),
        )
        .get_matches()
}

fn stdout_handler(msg: &FastMessage) {
    println!("{}", msg.data.d);
}

fn response_handler(msg: &FastMessage) -> Result<(), Error> {
    match msg.data.m.name.as_str() {
        "date" | "echo" | "yes" | "getobject" | "putobject" => {
            stdout_handler(msg)
        }
        _ => println!("Received {} response", msg.data.m.name),
    }

    Ok(())
}

fn main() {
    let matches = parse_opts(APP.to_string());
    let host = String::from(matches.value_of("host").unwrap_or(DEFAULT_HOST));
    let port = value_t!(matches, "port", u32).unwrap_or(DEFAULT_PORT);
    let addr = [host, String::from(":"), port.to_string()]
        .concat()
        .parse::<SocketAddr>()
        .unwrap_or_else(|e| {
            eprintln!(
                "Failed to parse host and port as valid socket address: \
                 {}",
                e
            );
            process::exit(1)
        });
    let method =
        String::from(matches.value_of("method").unwrap_or_else(|| {
            eprintln!("Failed to parse method argument as String");
            process::exit(1)
        }));
    let args = value_t!(matches, "args", Value).unwrap_or_else(|e| e.exit());

    let mut stream = TcpStream::connect(&addr).unwrap_or_else(|e| {
        eprintln!("Failed to connect to server: {}", e);
        process::exit(1)
    });

    let mut msg_id = FastMessageId::new();

    let result = client::send(method, args, &mut msg_id, &mut stream).and_then(
        |_bytes_written| client::receive(&mut stream, response_handler),
    );

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }
}
