/*
 * Copyright 2018 Joyent, Inc.
 */

extern crate serde_json;

use std::{io, str, usize};
use std::io::{Error, ErrorKind};
use std::time::{SystemTime, UNIX_EPOCH};

use bytes::{BufMut, BytesMut};
use byteorder::{ByteOrder, BigEndian};
use crc16::*;
use num::{FromPrimitive, ToPrimitive};
use serde_json::Value;
use tokio_io::_tokio_codec::{Encoder, Decoder};


/*
 * Message IDs: each Fast message has a message id, which is scoped to the Fast
 * connection.  We allocate these sequentially from a circular 31-bit space.
 */
// const FP_MSGID_MAX: u32 = i32::max_value() as u32;

const FP_OFF_TYPE: usize    = 0x1;
const FP_OFF_STATUS: usize  = 0x2;
const FP_OFF_MSGID: usize   = 0x3;
const FP_OFF_CRC: usize     = 0x7;
const FP_OFF_DATALEN: usize = 0xb;
const FP_OFF_DATA: usize    = 0xf;

const FP_HEADER_SZ: usize = FP_OFF_DATA;

const FP_VERSION_1: u8       = 0x1;
const FP_VERSION_CURRENT: u8 = FP_VERSION_1;

#[derive(FromPrimitive, ToPrimitive)]
pub enum FastMessageType {
    Json = 1
}

#[derive(FromPrimitive, ToPrimitive)]
pub enum FastMessageStatus {
    Data = 1,
    End = 2,
    Error = 3
}

pub struct FastMessageHeader {
    msg_type: FastMessageType,
    status: FastMessageStatus,
    id: u32,
    crc: u32,
    data_len: usize
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FastMessageMetaData {
    pub uts: u64,
    pub name: String
}

impl FastMessageMetaData {
    pub fn new(n: String) -> FastMessageMetaData {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let now_micros = now.as_secs() * 1_000_000
            + now.subsec_micros() as u64;

        FastMessageMetaData {
            uts: now_micros,
            name: n
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FastMessageData {
    pub m: FastMessageMetaData,
    pub d: Value
}

impl FastMessageData {
    pub fn new(n: String, d: Value) -> FastMessageData {
        FastMessageData {
            m: FastMessageMetaData::new(n),
            d: d
        }
    }
}

pub struct FastMessage {
    pub msg_type: FastMessageType,
    pub status: FastMessageStatus,
    pub id: u32,
    pub data: FastMessageData
}

impl FastMessage {
    pub fn parse(buf: &[u8]) -> Result<FastMessage, io::Error> {
        FastMessage::check_buffer_size(buf)?;
        let header = FastMessage::parse_header(buf)?;

        FastMessage::validate_data_length(buf, header.data_len)?;
        let raw_data = &buf[FP_OFF_DATA..FP_OFF_DATA + header.data_len];
        FastMessage::validate_crc(raw_data, header.crc)?;
        let data = FastMessage::parse_data(raw_data)?;


        Ok(FastMessage {
            msg_type: header.msg_type,
            status: header.status,
            id: header.id,
            data: data
        })
    }

    fn check_buffer_size(buf: &[u8]) -> Result<(), io::Error> {
        if buf.len() < FP_HEADER_SZ {
            let msg = "Unable to parse message: not enough bytes";
            Err(Error::new(ErrorKind::Other, msg))
        } else {
            Ok(())
        }
    }

    fn parse_header(buf: &[u8]) -> Result<FastMessageHeader, io::Error> {
        let msg_type = FromPrimitive::from_u8(buf[FP_OFF_TYPE])
            .ok_or_else(|| {
                let msg = "Failed to parse message type";
                Error::new(ErrorKind::Other, msg)
            })?;
        let status = FromPrimitive::from_u8(buf[FP_OFF_STATUS])
            .ok_or_else(|| {
                let msg = "Failed to parse message status";
                Error::new(ErrorKind::Other, msg)
            })?;
        let msg_id = BigEndian::read_u32(&buf[FP_OFF_MSGID..FP_OFF_MSGID+4]);
        let expected_crc = BigEndian::read_u32(&buf[FP_OFF_CRC..FP_OFF_CRC+4]);
        let data_len = BigEndian::read_u32(&buf[FP_OFF_DATALEN..FP_OFF_DATALEN+4]) as usize;

        Ok(FastMessageHeader {
            msg_type: msg_type,
            status: status,
            id: msg_id,
            crc: expected_crc,
            data_len: data_len
        })
    }

    fn validate_data_length(buf: &[u8], data_length: usize) -> Result<(), io::Error> {
        if buf.len() < (FP_HEADER_SZ + data_length) {
            let msg = "Data payload size does not match indicated data length";
            Err(Error::new(ErrorKind::Other, msg))
        } else {
            Ok(())
        }
    }

    fn validate_crc(data_buf: &[u8], crc: u32) -> Result<(), io::Error> {
        let calculated_crc = State::<ARC>::calculate(data_buf) as u32;
        if crc != calculated_crc {
            // Oops, node-fast uses an old version of a crc lib with bug so just
            // ignore a mismatch for now.
            // For interop to work we either need to port the buggy version of
            // the crc calculation over to rust or update node-fast to use an
            // updated version of the crc library.
            //
            // let msg = "Calculated CRC does not match the provided CRC";
            // Err(Error::new(ErrorKind::Other, msg))
            Ok(())
        } else {
            Ok(())
        }
    }

    fn parse_data(data_buf: &[u8]) -> Result<FastMessageData, io::Error> {
        match str::from_utf8(data_buf) {
            Ok(data_str) => serde_json::from_str(data_str)
                .map_err(|_e| {
                    let msg = "Failed to parse data payload as JSON";
                    Error::new(ErrorKind::Other, msg)
                }),
            Err(_) => {
                let msg = "Failed to parse data payload as UTF-8";
                Err(Error::new(ErrorKind::Other, msg))
            }
        }
    }

    pub fn data(msg_id: u32, data: FastMessageData) -> FastMessage {
        FastMessage {
            msg_type: FastMessageType::Json,
            status: FastMessageStatus::Data,
            id: msg_id,
            data: data
        }
    }

    pub fn end(msg_id: u32) -> FastMessage {
        FastMessage {
            msg_type: FastMessageType::Json,
            status: FastMessageStatus::End,
            id: msg_id,
            data: FastMessageData::new(String::from("yes"), Value::Array(vec![]))
        }
    }
}

pub struct FastRpc;

impl Decoder for FastRpc {
    type Item = Vec<FastMessage>;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, io::Error> {
        if buf.len() > 0 {
            let parsed_msg = FastMessage::parse(&buf).map_err(|e| {
                let msg = format!("failed to parse Fast request: {:?}", e);
                io::Error::new(io::ErrorKind::Other, msg)
            })?;
            buf.clear();
            Ok(Some(vec![parsed_msg]))
        } else {
            Ok(None)
        }
    }
}


impl Encoder for FastRpc {
    type Item = Vec<FastMessage>;
    //TODO: Create custom FastMessage error type
    type Error = io::Error;
    fn encode(&mut self, item: Self::Item, buf: &mut BytesMut) -> Result<(), io::Error> {
        let results: Vec<Result<(), String>> = item.iter().map(|x| { encode_msg(x, buf) }).collect();
        let result: Result<Vec<()>, String> = results.iter().cloned().collect();
        match result {
            Ok(_) => Ok(()),
            Err(errs) => {
                Err(Error::new(ErrorKind::Other, errs))
            }
        }
    }
}

fn encode_msg(msg: &FastMessage, buf: &mut BytesMut) -> Result<(), String> {
    let m_msg_type_u8 = ToPrimitive::to_u8(&msg.msg_type);
    let m_status_u8 = ToPrimitive::to_u8(&msg.status);
    match (m_msg_type_u8, m_status_u8) {
        (Some(msg_type_u8), Some(status_u8)) => {
            let data_str = serde_json::to_string(&msg.data).unwrap();
            let data_len = data_str.len();
            let buf_capacity = buf.capacity();
            if FP_HEADER_SZ + data_len > buf_capacity {
                buf.reserve(FP_HEADER_SZ + data_len as usize - buf_capacity);
            }
            buf.put_u8(FP_VERSION_CURRENT);
            buf.put_u8(msg_type_u8);
            buf.put_u8(status_u8);
            buf.put_u32_be(msg.id);
            buf.put_u32_be(State::<ARC>::calculate(data_str.as_bytes()) as u32);
            buf.put_u32_be(data_str.len() as u32);
            buf.put(data_str);
            Ok(())
        },
        (None, Some(_)) =>
            Err(String::from("Invalid message type")),
        (Some(_), None) =>
            Err(String::from("Invalid status")),
        (None, None) =>
            Err(String::from("Invalid message type and status"))
    }
}
