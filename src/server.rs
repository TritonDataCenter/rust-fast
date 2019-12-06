// Copyright 2019 Joyent, Inc.

//! This module provides the interface for creating Fast servers.

use std::io::Error;

use serde_json::json;
use slog::{debug, error, o, Drain, Logger};
use tokio;
use tokio::codec::Decoder;
use tokio::net::TcpStream;
use tokio::prelude::*;

use crate::protocol::{FastMessage, FastMessageData, FastRpc};

/// Create a task to be used by the tokio runtime for handling responses to Fast
/// protocol requests.
pub fn make_task<F>(
    socket: TcpStream,
    mut response_handler: F,
    log: Option<&Logger>,
) -> impl Future<Item = (), Error = ()> + Send
where
    F: FnMut(&FastMessage, &Logger) -> Result<Vec<FastMessage>, Error> + Send,
{
    let (tx, rx) = FastRpc.framed(socket).split();

    // If no logger was provided use the slog StdLog drain by default
    let rx_log = log
        .cloned()
        .unwrap_or_else(|| Logger::root(slog_stdlog::StdLog.fuse(), o!()));

    let tx_log = rx_log.clone();
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

    for msg in msgs {
        match response_handler(&msg, &log) {
            Ok(mut response) => {
                // Make sure there is room in responses to fit another response plus an
                // end message
                let responses_len = responses.len();
                let response_len = response.len();
                let responses_capacity = responses.capacity();
                if responses_len + response_len > responses_capacity {
                    let needed_capacity =
                        responses_len + response_len - responses_capacity;
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
                let method = msg.data.m.name.clone();
                let value = json!({
                    "name": "FastError",
                    "message": err.to_string()
                });

                let err_msg = FastMessage::error(
                    msg.id,
                    FastMessageData::new(method, value),
                );
                responses.push(err_msg);
            }
        }
    }

    Box::new(future::ok(responses))
}
