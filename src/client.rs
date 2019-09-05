// Copyright 2019 Joyent, Inc.

//! This module provides the interface for creating Fast clients.

use std::io::{Error, ErrorKind};
use std::net::TcpStream;

use bytes::BytesMut;
use serde_json::Value;
use tokio::prelude::*;

use crate::protocol;
use crate::protocol::{
    FastMessage, FastMessageData, FastMessageId, FastMessageServerError,
    FastMessageStatus, FastParseError,
};

enum BufferAction {
    Keep,
    Trim(usize),
    Done,
}

/// Send a message to a Fast server using the provided TCP stream.
pub fn send(
    method: String,
    args: Value,
    msg_id: &mut FastMessageId,
    stream: &mut TcpStream,
) -> Result<usize, Error> {
    // It is safe to call unwrap on the msg_id iterator because the
    // implementation of Iterator for FastMessageId will only ever return
    // Some(id). The Option return type is required by the Iterator trait.
    let msg = FastMessage::data(
        msg_id.next().unwrap() as u32,
        FastMessageData::new(method, args),
    );
    let mut write_buf = BytesMut::new();
    match protocol::encode_msg(&msg, &mut write_buf) {
        Ok(_) => stream.write(write_buf.as_ref()),
        Err(err_str) => Err(Error::new(ErrorKind::Other, err_str)),
    }
}

/// Receive a message from a Fast server on the provided TCP stream and call
/// `response_handler` on the response.
pub fn receive<F>(
    stream: &mut TcpStream,
    mut response_handler: F,
) -> Result<usize, Error>
where
    F: FnMut(&FastMessage) -> Result<(), Error>,
{
    let mut stream_end = false;
    let mut msg_buf: Vec<u8> = Vec::new();
    let mut total_bytes = 0;
    let mut result = Ok(total_bytes);

    while !stream_end {
        let mut read_buf = [0; 128];
        match stream.read(&mut read_buf) {
            Ok(0) => {
                result = Err(Error::new(
                    ErrorKind::UnexpectedEof,
                    "Received EOF (0 bytes) from server",
                ));
                stream_end = true;
            }
            Ok(byte_count) => {
                total_bytes += byte_count;
                msg_buf.extend_from_slice(&read_buf[0..byte_count]);
                match parse_and_handle_messages(
                    msg_buf.as_slice(),
                    &mut response_handler,
                ) {
                    Ok(BufferAction::Keep) => (),
                    Ok(BufferAction::Trim(rest_offset)) => {
                        let truncate_bytes = msg_buf.len() - rest_offset;
                        msg_buf.rotate_left(rest_offset);
                        msg_buf.truncate(truncate_bytes);
                        result = Ok(total_bytes);
                    }
                    Ok(BufferAction::Done) => stream_end = true,
                    Err(e) => {
                        result = Err(e);
                        stream_end = true
                    }
                }
            }
            Err(err) => {
                result = Err(err);
                stream_end = true
            }
        }
    }
    result
}

fn parse_and_handle_messages<F>(
    read_buf: &[u8],
    response_handler: &mut F,
) -> Result<BufferAction, Error>
where
    F: FnMut(&FastMessage) -> Result<(), Error>,
{
    let mut offset = 0;
    let mut done = false;

    let mut result = Ok(BufferAction::Keep);

    while !done {
        match FastMessage::parse(&read_buf[offset..]) {
            Ok(ref fm) if fm.status == FastMessageStatus::End => {
                result = Ok(BufferAction::Done);
                done = true;
            }
            Ok(fm) => {
                offset += fm.msg_size.unwrap();
                match fm.status {
                    FastMessageStatus::Data | FastMessageStatus::End => {
                        if let Err(e) = response_handler(&fm) {
                            result = Err(e);
                            done = true;
                        } else {
                            result = Ok(BufferAction::Trim(offset));
                        }
                    }
                    FastMessageStatus::Error => {
                        result = serde_json::from_value(fm.data.d)
                            .or_else(|_| Err(unspecified_error().into()))
                            .and_then(
                                |e: FastMessageServerError| Err(e.into()),
                            );

                        done = true;
                    }
                }
            }
            Err(FastParseError::NotEnoughBytes(_bytes)) => {
                done = true;
            }
            Err(FastParseError::IOError(e)) => {
                result = Err(e);
                done = true;
            }
        }
    }

    result
}

fn unspecified_error() -> FastMessageServerError {
    FastMessageServerError::new(
        "UnspecifiedServerError",
        "Server reported unspecified error.",
    )
}
