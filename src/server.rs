/*
 * Copyright 2018 Joyent, Inc.
 */

use std::io::{Error, ErrorKind};
use std::sync::Arc;

use slog::Logger;
use tokio;
use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::codec::Decoder;

use protocol::{FastRpc, FastMessage};


pub fn process(socket: TcpStream,
               response_handler: Arc<(Fn(&FastMessage, &Logger) -> Result<Vec<FastMessage>, Error> + Send + Sync)>,
               log: &Logger
)
{
    let (tx, rx) = FastRpc.framed(socket).split();
    let rx_log = log.clone();
    let tx_log = log.clone();
    let task = tx.send_all(rx.and_then(
        move |x| {
            debug!(rx_log, "processing fast message");
            let c = Arc::clone(&response_handler);
            respond(x, c, &rx_log)
        }))
        .then(move |res| {
            if let Err(e) = res {
                error!(tx_log, "failed to process connection"; "err" => %e);
                //TODO: Send error response to client
            }

            debug!(tx_log, "transmitted response to client");
            Ok(())
        });

    // Spawn the task that handles the connection.
    tokio::spawn(task);
}

pub fn respond(msgs: Vec<FastMessage>,
               response_handler: Arc<(Fn(&FastMessage, &Logger) -> Result<Vec<FastMessage>, Error> + Sync + Send)>,
               log: &Logger)
               -> impl Future<Item = Vec<FastMessage>, Error = Error> + Send
{
    match msgs.get(0) {
        Some(msg) => {
            match response_handler(msg, &log) {
                Ok(mut response) => {
                    debug!(log, "generated response");
                    let method = msg.data.m.name.clone();
                    response.push(FastMessage::end(msg.id, method));
                    Box::new(future::ok(response))
                },
                Err(err) => Box::new(future::err(err))
            }
        },
        None => {
            Box::new(future::err(Error::new(ErrorKind::Other, "no message available")))
        }
    }
}
