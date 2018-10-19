/*
 * Copyright 2018 Joyent, Inc.
 */

use std::io::{Error, ErrorKind};
use std::sync::Arc;
use std::net::TcpStream;

use bytes::BytesMut;
use serde_json::Value;
use tokio::prelude::*;

use protocol;
use protocol::{FastMessage, FastMessageData, FastMessageStatus, FastParseError};

enum BufferAction {
    Keep,
    Trim(usize),
    Done
}

pub fn send(method: String,
            args: Value,
            stream: &mut TcpStream) -> Result<usize, Error>
{
    //TODO: Replace hardcoded msg id
    let msg = FastMessage::data(0x1, FastMessageData::new(method, args));
    let mut write_buf = BytesMut::new();
    match protocol::encode_msg(&msg, &mut write_buf) {
        Ok(_) => stream.write(write_buf.as_ref()),
        Err(err_str) => Err(Error::new(ErrorKind::Other, err_str))
    }
}

pub fn receive(stream: &mut TcpStream,
               response_handler: Arc<(Fn(&FastMessage) -> Result<(), Error>)>)
               -> Result<usize, Error>
{
    let mut stream_end = false;
    let mut msg_buf: Vec<u8> = Vec::new();
    let mut total_bytes = 0;
    let mut result = Ok(total_bytes);

    while stream_end == false {
        let mut read_buf = [0; 128];
        match stream.read(&mut read_buf) {
            Ok(byte_count) => {
                total_bytes += byte_count;
                msg_buf.extend_from_slice(&read_buf[0..byte_count]);
                match parse_and_handle_messages(msg_buf.as_slice(), Arc::clone(&response_handler)) {
                    Ok(BufferAction::Keep) => (),
                    Ok(BufferAction::Trim(rest_offset)) => {
                        let truncate_bytes = msg_buf.len() - rest_offset;
                        msg_buf.rotate_left(rest_offset);
                        msg_buf.truncate(truncate_bytes);
                        result = Ok(total_bytes);
                    },
                    Ok(BufferAction::Done) => stream_end = true,
                    Err(e) => {
                        result = Err(e);
                        stream_end = true
                    }
                }
            },
            Err(err) => {
                result = Err(err);
                stream_end = true
            }
        }
    }
    result
}

fn parse_and_handle_messages(read_buf: &[u8],
                             response_handler: Arc<(Fn(&FastMessage) -> Result<(), Error>)>)
                             -> Result<BufferAction, Error>
{
    let mut offset = 0;
    let mut done = false;

    let mut result = Ok(BufferAction::Keep);

    while done == false {
        match FastMessage::parse(&read_buf[offset..]) {
            Ok(ref fm) if fm.status == FastMessageStatus::End => {
                result = Ok(BufferAction::Done);
                done = true;
            },
            Ok(fm) => {
                offset += fm.msg_size.unwrap();
                let _ = response_handler(&fm);
                result = Ok(BufferAction::Trim(offset));
            },
            Err(FastParseError::NotEnoughBytes(_bytes)) => {
                done = true;
            },
            Err(FastParseError::IOError(e)) => {
                result = Err(e);
                done = true;
            }
        }
    }

    result
}
