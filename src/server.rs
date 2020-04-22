// Copyright 2020 Joyent, Inc.

//! This module provides the interface for creating Fast servers.

use std::error::Error as StdError;
use std::io::Error;

use futures::SinkExt;
use serde_json::json;
use slog::{debug, o, Drain, Logger};
use tokio::net::TcpStream;
use tokio::stream::StreamExt;
use tokio_util::codec::Framed;

use crate::protocol::{FastMessage, FastMessageData, FastRpc};

/// Create a task to be used by the tokio runtime for handling responses to Fast
/// protocol requests.
pub async fn make_task<F>(
    stream: TcpStream,
    response_handler: F,
    log: Option<&Logger>,
) where
    F: FnMut(&FastMessage, &Logger) -> Result<Vec<FastMessage>, Error> + Send,
{
    if let Err(e) = process(stream, response_handler, log).await {
        println!("failed to process connection; error = {}", e);
    }
}

async fn process<F>(
    stream: TcpStream,
    mut response_handler: F,
    log: Option<&Logger>,
) -> Result<(), Box<dyn StdError>>
where
    F: FnMut(&FastMessage, &Logger) -> Result<Vec<FastMessage>, Error> + Send,
{
    let mut transport = Framed::new(stream, FastRpc);

    while let Some(request) = transport.next().await {
        match request {
            Ok(request) => {
                let rx_log = log.cloned().unwrap_or_else(|| {
                    Logger::root(slog_stdlog::StdLog.fuse(), o!())
                });
                debug!(rx_log, "processing fast message");
                let response =
                    respond(request, &mut response_handler, &rx_log).await?;
                transport.send(response).await?;
            }
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

async fn respond<F>(
    msgs: Vec<FastMessage>,
    response_handler: &mut F,
    log: &Logger,
) -> Result<Vec<FastMessage>, Box<dyn StdError>>
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

    Ok(responses)
}
