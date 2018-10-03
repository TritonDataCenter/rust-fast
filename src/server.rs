/*
 * Copyright 2018 Joyent, Inc.
 */

extern crate tokio;

use std::io::{Error, ErrorKind};
use std::sync::Arc;

use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::codec::Decoder;

use protocol::{FastRpc, FastMessage};


pub fn process(socket: TcpStream,
               response_handler: Arc<(Fn(&FastMessage) -> Result<Vec<FastMessage>, Error> + Send + Sync)>)
{
    let (tx, rx) = FastRpc.framed(socket).split();
    let task = tx.send_all(rx.and_then(
        move |x| {
            let c = Arc::clone(&response_handler);
            respond(x, c)
        }))
        .then(|res| {
            if let Err(e) = res {
                println!("failed to process connection; error = {:?}", e);
            }

            Ok(())
        });

    // Spawn the task that handles the connection.
    tokio::spawn(task);
}

pub fn respond(msgs: Vec<FastMessage>,
               response_handler: Arc<(Fn(&FastMessage) -> Result<Vec<FastMessage>, Error> + Sync + Send)>)
               -> Box<Future<Item = Vec<FastMessage>, Error = Error> + Send>
{
    match msgs.get(0) {
        Some(msg) => {
            match response_handler(msg) {
                Ok(mut response) => {
                    response.push(FastMessage::end(msg.id));
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
