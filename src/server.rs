/*
 * Copyright 2019 Joyent, Inc.
 */

use std::io::Error;

use slog::{debug, error, Logger};
use tokio;
use tokio::codec::Decoder;
use tokio::net::TcpStream;
use tokio::prelude::*;

use crate::protocol::{FastMessage, FastRpc};

/// Create a task to be used by the tokio runtime for response handling for Fast
/// message requests.
pub fn make_task<F>(
    socket: TcpStream,
    mut response_handler: F,
    log: &Logger,
) -> impl Future<Item = (), Error = ()> + Send
where
    F: FnMut(&FastMessage, &Logger) -> Result<Vec<FastMessage>, Error> + Send,
{
    let (tx, rx) = FastRpc.framed(socket).split();
    let rx_log = log.clone();
    let tx_log = log.clone();
    tx.send_all(rx.and_then(move |x| {
        debug!(rx_log, "processing fast message");
        respond(x, &mut response_handler, &rx_log)
    }))
    .then(move |res| {
        if let Err(e) = res {
            error!(tx_log, "failed to process connection"; "err" => %e);
        }

        debug!(tx_log, "transmitted response to client");
        Ok(())
    })
}

fn respond<F>(
    msgs: Vec<FastMessage>,
    response_handler: &mut F,
    log: &Logger,
) -> impl Future<Item = Vec<FastMessage>, Error = Error> + Send
where
    F: FnMut(&FastMessage, &Logger) -> Result<Vec<FastMessage>, Error> + Send,
{
    debug!(log, "responding to {} messages", msgs.len());

    let mut responses: Vec<FastMessage> = Vec::new();
    let mut error: Option<Error> = None;

    for msg in msgs {
        match response_handler(&msg, &log) {
            Ok(mut response) => {
                // Make sure there is room in responses to fit another response plus an
                // end message
                let responses_len = responses.len();
                let response_len = response.len();
                let responses_capacity = responses.capacity();
                if responses_len + response_len > responses_capacity {
                    let needed_capacity = responses_len + response_len - responses_capacity;
                    responses.reserve(needed_capacity);
                }

                // Add all response messages for this message to the vector of
                // all responses
                response.drain(..).for_each(|r| {
                    responses.push(r);
                });

                debug!(log, "generated response");
                let method = msg.data.m.name.clone();
                responses.push(FastMessage::end(msg.id, method));
            }
            Err(err) => {
                error = Some(err);
            }
        }
    }

    let fut = if let Some(err) = error {
        future::err(err)
    } else {
        future::ok(responses)
    };

    Box::new(fut)
}
