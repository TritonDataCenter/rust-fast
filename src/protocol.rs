// Copyright 2020 Joyent, Inc.

//! This module contains the types and functions used to encode and decode Fast
//! messages. The contents of this module are not needed for normal client or
//! server consumers of this crate, but they are exposed for the special case of
//! someone needing to implement custom client or server code.

use std::io::{Error, ErrorKind};
use std::sync::atomic::AtomicUsize;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{io, str, usize};

use byteorder::{BigEndian, ByteOrder};
use bytes::{BufMut, BytesMut};
use crc16::*;
use num::{FromPrimitive, ToPrimitive};
use num_derive::{FromPrimitive, ToPrimitive};
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use tokio_io::_tokio_codec::{Decoder, Encoder};

const FP_OFF_TYPE: usize = 0x1;
const FP_OFF_STATUS: usize = 0x2;
const FP_OFF_MSGID: usize = 0x3;
const FP_OFF_CRC: usize = 0x7;
const FP_OFF_DATALEN: usize = 0xb;
const FP_OFF_DATA: usize = 0xf;

/// The size of a Fast message header
pub const FP_HEADER_SZ: usize = FP_OFF_DATA;

const FP_VERSION_2: u8 = 0x2;
const FP_VERSION_CURRENT: u8 = FP_VERSION_2;

/// A data type representing a Fast message id that can safely be shard between
/// threads. The `next` associated function retrieves the next id value and
/// manages the circular message id space internally.
#[derive(Default)]
pub struct FastMessageId(AtomicUsize);

impl FastMessageId {
    /// Creates a new FastMessageId
    pub fn new() -> Self {
        FastMessageId(AtomicUsize::new(0x0))
    }
}

impl Iterator for FastMessageId {
    type Item = usize;

    /// Returns the next Fast message id and increments the value modulo the
    /// usize MAX_VALUE - 1.
    fn next(&mut self) -> Option<Self::Item> {
        // Increment our count. This is why we started at zero.
        let id_value = self.0.get_mut();
        let current = *id_value;
        *id_value = (*id_value + 1) % (usize::max_value() - 1);

        Some(current)
    }
}

/// An error type representing a failure to parse a buffer as a Fast message.
#[derive(Debug)]
pub enum FastParseError {
    NotEnoughBytes(usize),
    IOError(Error),
}

impl From<io::Error> for FastParseError {
    fn from(error: io::Error) -> Self {
        FastParseError::IOError(error)
    }
}

impl From<FastParseError> for Error {
    fn from(pfr: FastParseError) -> Self {
        match pfr {
            FastParseError::NotEnoughBytes(_) => {
                let msg = "Unable to parse message: not enough bytes";
                Error::new(ErrorKind::Other, msg)
            }
            FastParseError::IOError(e) => e,
        }
    }
}

/// An error type representing Fast error messages that may be returned from a
/// Fast server.
#[derive(Debug, Deserialize, Serialize)]
pub struct FastMessageServerError {
    pub name: String,
    pub message: String,
}

impl FastMessageServerError {
    pub fn new(name: &str, message: &str) -> Self {
        FastMessageServerError {
            name: String::from(name),
            message: String::from(message),
        }
    }
}

impl From<FastMessageServerError> for Error {
    fn from(err: FastMessageServerError) -> Self {
        Error::new(ErrorKind::Other, format!("{}: {}", err.name, err.message))
    }
}

/// Represents the Type field of a Fast message. Currently there is only one
/// valid value, JSON.
#[derive(Debug, FromPrimitive, ToPrimitive, PartialEq, Clone)]
pub enum FastMessageType {
    Json = 1,
}

/// Represents the Status field of a Fast message.
#[derive(Debug, FromPrimitive, ToPrimitive, PartialEq, Clone)]
pub enum FastMessageStatus {
    Data = 1,
    End = 2,
    Error = 3,
}

/// This type encapsulates the header of a Fast message.
pub struct FastMessageHeader {
    /// The Type field of the Fast message
    msg_type: FastMessageType,
    /// The Status field of the Fast message
    status: FastMessageStatus,
    /// The Fast message identifier
    id: u32,
    /// The CRC16 check value of the Fast message data payload
    crc: u32,
    /// The length in bytes of the Fast message data payload
    data_len: usize,
}

/// Represents the metadata about a `FastMessage` data payload. This includes a
/// timestamp and an RPC method name.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct FastMessageMetaData {
    pub uts: u64,
    pub name: String,
}

impl FastMessageMetaData {
    pub fn new(n: String) -> FastMessageMetaData {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let now_micros =
            now.as_secs() * 1_000_000 + u64::from(now.subsec_micros());

        FastMessageMetaData {
            uts: now_micros,
            name: n,
        }
    }
}

/// Encapsulates the Fast message metadata and the JSON formatted message data.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct FastMessageData {
    pub m: FastMessageMetaData,
    pub d: Value,
}

impl FastMessageData {
    pub fn new(n: String, d: Value) -> FastMessageData {
        FastMessageData {
            m: FastMessageMetaData::new(n),
            d,
        }
    }
}

/// Represents a Fast message including the header and data payload
#[derive(Debug, Clone)]
pub struct FastMessage {
    /// The Type field of the Fast message
    pub msg_type: FastMessageType,
    /// The Status field of the Fast message
    pub status: FastMessageStatus,
    /// The Fast message identifier
    pub id: u32,
    /// The length in bytes of the Fast message data payload
    pub msg_size: Option<usize>,
    /// The data payload of the Fast message
    pub data: FastMessageData,
}

impl PartialEq for FastMessage {
    fn eq(&self, other: &FastMessage) -> bool {
        self.msg_type == other.msg_type
            && self.status == other.status
            && self.id == other.id
            && self.msg_size == other.msg_size
            && self.data == other.data
    }
}

impl FastMessage {
    /// Parse a byte buffer into a `FastMessage`. Returns a `FastParseError` if
    /// the available bytes cannot be parsed to a `FastMessage`.
    pub fn parse(buf: &[u8]) -> Result<FastMessage, FastParseError> {
        FastMessage::check_buffer_size(buf)?;
        let header = FastMessage::parse_header(buf)?;

        FastMessage::validate_data_length(buf, header.data_len)?;
        let raw_data = &buf[FP_OFF_DATA..FP_OFF_DATA + header.data_len];
        FastMessage::validate_crc(raw_data, header.crc)?;
        let data = FastMessage::parse_data(raw_data)?;

        let msg_size = match header.status {
            FastMessageStatus::End => None,
            _ => Some(FP_OFF_DATA + header.data_len),
        };

        Ok(FastMessage {
            msg_type: header.msg_type,
            status: header.status,
            id: header.id,
            msg_size,
            data,
        })
    }

    /// Check that the provided byte buffer contains at least `FP_HEADER_SZ`
    /// bytes.  Returns a `FastParseError` if this is not the case.
    pub fn check_buffer_size(buf: &[u8]) -> Result<(), FastParseError> {
        if buf.len() < FP_HEADER_SZ {
            Err(FastParseError::NotEnoughBytes(buf.len()))
        } else {
            Ok(())
        }
    }

    /// Parse a portion of a byte buffer into a `FastMessageHeader`. Returns a
    /// `FastParseError` if the available bytes cannot be parsed to a
    /// `FastMessageHeader`.
    pub fn parse_header(
        buf: &[u8],
    ) -> Result<FastMessageHeader, FastParseError> {
        let msg_type =
            FromPrimitive::from_u8(buf[FP_OFF_TYPE]).ok_or_else(|| {
                let msg = "Failed to parse message type";
                FastParseError::IOError(Error::new(ErrorKind::Other, msg))
            })?;
        let status =
            FromPrimitive::from_u8(buf[FP_OFF_STATUS]).ok_or_else(|| {
                let msg = "Failed to parse message status";
                FastParseError::IOError(Error::new(ErrorKind::Other, msg))
            })?;
        let msg_id = BigEndian::read_u32(&buf[FP_OFF_MSGID..FP_OFF_MSGID + 4]);
        let expected_crc =
            BigEndian::read_u32(&buf[FP_OFF_CRC..FP_OFF_CRC + 4]);
        let data_len =
            BigEndian::read_u32(&buf[FP_OFF_DATALEN..FP_OFF_DATALEN + 4])
                as usize;

        Ok(FastMessageHeader {
            msg_type,
            status,
            id: msg_id,
            crc: expected_crc,
            data_len,
        })
    }

    fn validate_data_length(
        buf: &[u8],
        data_length: usize,
    ) -> Result<(), FastParseError> {
        if buf.len() < (FP_HEADER_SZ + data_length) {
            Err(FastParseError::NotEnoughBytes(buf.len()))
        } else {
            Ok(())
        }
    }

    fn validate_crc(data_buf: &[u8], crc: u32) -> Result<(), FastParseError> {
        let calculated_crc = u32::from(State::<ARC>::calculate(data_buf));
        if crc != calculated_crc {
            let msg = "Calculated CRC does not match the provided CRC";
            Err(FastParseError::IOError(Error::new(ErrorKind::Other, msg)))
        } else {
            Ok(())
        }
    }

    fn parse_data(data_buf: &[u8]) -> Result<FastMessageData, FastParseError> {
        match str::from_utf8(data_buf) {
            Ok(data_str) => serde_json::from_str(data_str).map_err(|_e| {
                let msg = "Failed to parse data payload as JSON";
                FastParseError::IOError(Error::new(ErrorKind::Other, msg))
            }),
            Err(_) => {
                let msg = "Failed to parse data payload as UTF-8";
                Err(FastParseError::IOError(Error::new(ErrorKind::Other, msg)))
            }
        }
    }

    /// Returns a `FastMessage` that represents a Fast protocol `DATA` message
    /// with the provided message identifer and data payload.
    pub fn data(msg_id: u32, data: FastMessageData) -> FastMessage {
        FastMessage {
            msg_type: FastMessageType::Json,
            status: FastMessageStatus::Data,
            id: msg_id,
            msg_size: None,
            data,
        }
    }

    /// Returns a `FastMessage` that represents a Fast protocol `END` message
    /// with the provided message identifer. The method parameter is used in the
    /// otherwise empty data payload.
    pub fn end(msg_id: u32, method: String) -> FastMessage {
        FastMessage {
            msg_type: FastMessageType::Json,
            status: FastMessageStatus::End,
            id: msg_id,
            msg_size: None,
            data: FastMessageData::new(method, Value::Array(vec![])),
        }
    }

    /// Returns a `FastMessage` that represents a Fast protocol `ERROR` message
    /// with the provided message identifer and data payload.
    pub fn error(msg_id: u32, data: FastMessageData) -> FastMessage {
        FastMessage {
            msg_type: FastMessageType::Json,
            status: FastMessageStatus::Error,
            id: msg_id,
            msg_size: None,
            data,
        }
    }
}

/// This type implements the functions necessary for the Fast protocl framing.
pub struct FastRpc;

impl Decoder for FastRpc {
    type Item = Vec<FastMessage>;
    type Error = Error;

    fn decode(
        &mut self,
        buf: &mut BytesMut,
    ) -> Result<Option<Self::Item>, Error> {
        let mut msgs: Self::Item = Vec::new();
        let mut done = false;

        while !done && !buf.is_empty() {
            // Make sure there is room in msgs to fit a message
            if msgs.len() + 1 > msgs.capacity() {
                msgs.reserve(1);
            }

            match FastMessage::parse(&buf) {
                Ok(parsed_msg) => {
                    // TODO: Handle the error case here!
                    let data_str =
                        serde_json::to_string(&parsed_msg.data).unwrap();
                    let data_len = data_str.len();
                    buf.advance(FP_HEADER_SZ + data_len);
                    msgs.push(parsed_msg);
                    Ok(())
                }
                Err(FastParseError::NotEnoughBytes(_)) => {
                    // Not enough bytes available yet so we need to return
                    // Ok(None) to let the Framed instance know to read more
                    // data before calling this function again.
                    done = true;
                    Ok(())
                }
                Err(err) => {
                    let msg = format!(
                        "failed to parse Fast request: {}",
                        Error::from(err)
                    );
                    Err(Error::new(ErrorKind::Other, msg))
                }
            }?
        }

        if msgs.is_empty() {
            Ok(None)
        } else {
            Ok(Some(msgs))
        }
    }
}

impl Encoder for FastRpc {
    type Item = Vec<FastMessage>;
    //TODO: Create custom FastMessage error type
    type Error = io::Error;
    fn encode(
        &mut self,
        item: Self::Item,
        buf: &mut BytesMut,
    ) -> Result<(), io::Error> {
        let results: Vec<Result<(), String>> =
            item.iter().map(|x| encode_msg(x, buf)).collect();
        let result: Result<Vec<()>, String> = results.iter().cloned().collect();
        match result {
            Ok(_) => Ok(()),
            Err(errs) => Err(Error::new(ErrorKind::Other, errs)),
        }
    }
}

/// Encode a `FastMessage` into a byte buffer. The `Result` contains a unit type
/// on success and an error string on failure.
pub(crate) fn encode_msg(
    msg: &FastMessage,
    buf: &mut BytesMut,
) -> Result<(), String> {
    let m_msg_type_u8 = msg.msg_type.to_u8();
    let m_status_u8 = msg.status.to_u8();
    match (m_msg_type_u8, m_status_u8) {
        (Some(msg_type_u8), Some(status_u8)) => {
            // TODO: Handle the error case here!
            let data_str = serde_json::to_string(&msg.data).unwrap();
            let data_len = data_str.len();
            let buf_capacity = buf.capacity();
            if buf.len() + FP_HEADER_SZ + data_len > buf_capacity {
                buf.reserve(FP_HEADER_SZ + data_len as usize);
            }
            buf.put_u8(FP_VERSION_CURRENT);
            buf.put_u8(msg_type_u8);
            buf.put_u8(status_u8);
            buf.put_u32_be(msg.id);
            buf.put_u32_be(u32::from(State::<ARC>::calculate(
                data_str.as_bytes(),
            )));
            buf.put_u32_be(data_str.len() as u32);
            buf.put(data_str);
            Ok(())
        }
        (None, Some(_)) => Err(String::from("Invalid message type")),
        (Some(_), None) => Err(String::from("Invalid status")),
        (None, None) => Err(String::from("Invalid message type and status")),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::iter;

    use quickcheck::{quickcheck, Arbitrary, Gen};
    use rand::distributions::Alphanumeric;
    use rand::seq::SliceRandom;
    use rand::Rng;
    use serde_json::Map;

    fn random_string<G: Gen>(g: &mut G, len: usize) -> String {
        iter::repeat(())
            .map(|()| g.sample(Alphanumeric))
            .take(len)
            .collect()
    }

    fn nested_object<G: Gen>(g: &mut G) -> Value {
        let k_len = g.gen::<u8>() as usize;
        let v_len = g.gen::<u8>() as usize;
        let k = random_string(g, k_len);
        let v = random_string(g, v_len);
        let count = g.gen::<u64>();
        let mut inner_obj = Map::new();
        let mut outer_obj = Map::new();
        let _ = inner_obj.insert(k, Value::String(v));
        outer_obj
            .insert(String::from("value"), Value::Object(inner_obj))
            .and_then(|_| {
                outer_obj.insert(String::from("count"), count.into())
            });
        Value::Object(outer_obj)
    }

    #[derive(Clone, Debug)]
    struct MessageCount(u8);

    impl Arbitrary for MessageCount {
        fn arbitrary<G: Gen>(g: &mut G) -> MessageCount {
            let mut c = 0;
            while c == 0 {
                c = g.gen::<u8>()
            }

            MessageCount(c)
        }
    }

    impl Arbitrary for FastMessageStatus {
        fn arbitrary<G: Gen>(g: &mut G) -> FastMessageStatus {
            let choices = [
                FastMessageStatus::Data,
                FastMessageStatus::End,
                FastMessageStatus::Error,
            ];

            choices.choose(g).unwrap().clone()
        }
    }

    impl Arbitrary for FastMessageType {
        fn arbitrary<G: Gen>(g: &mut G) -> FastMessageType {
            let choices = [FastMessageType::Json];

            choices.choose(g).unwrap().clone()
        }
    }

    impl Arbitrary for FastMessageMetaData {
        fn arbitrary<G: Gen>(g: &mut G) -> FastMessageMetaData {
            let name = random_string(g, 10);
            FastMessageMetaData::new(name)
        }
    }

    impl Arbitrary for FastMessageData {
        fn arbitrary<G: Gen>(g: &mut G) -> FastMessageData {
            let md = FastMessageMetaData::arbitrary(g);

            let choices = [
                Value::Array(vec![]),
                Value::Object(Map::new()),
                nested_object(g),
                Value::Array(vec![nested_object(g)]),
            ];

            let value = choices.choose(g).unwrap().clone();

            FastMessageData { m: md, d: value }
        }
    }

    impl Arbitrary for FastMessage {
        fn arbitrary<G: Gen>(g: &mut G) -> FastMessage {
            let msg_type = FastMessageType::arbitrary(g);
            let status = FastMessageStatus::arbitrary(g);
            let id = g.gen::<u32>();

            let data = FastMessageData::arbitrary(g);
            let data_str = serde_json::to_string(&data).unwrap();
            let msg_sz = match status {
                FastMessageStatus::End => None,
                _ => Some(FP_OFF_DATA + data_str.len()),
            };

            FastMessage {
                msg_type,
                status,
                id,
                msg_size: msg_sz,
                data,
            }
        }
    }

    quickcheck! {
        fn prop_fast_message_roundtrip(msg: FastMessage) -> bool {
            let mut write_buf = BytesMut::new();
            match encode_msg(&msg, &mut write_buf) {
                Ok(_) => {
                    match FastMessage::parse(&write_buf) {
                        Ok(decoded_msg) => decoded_msg == msg,
                        Err(_) => false
                    }
                },
                Err(_) => false
            }
        }
    }

    quickcheck! {
        fn prop_fast_message_bundling(msg: FastMessage, msg_count: MessageCount) -> bool {
            let mut write_buf = BytesMut::new();
            let mut error_occurred = false;
            for _ in 0..msg_count.0 {
                match encode_msg(&msg, &mut write_buf) {
                    Ok(_) => (),
                    Err(_) => {
                        error_occurred = true;
                    }
                }
            }

            if error_occurred {
                return false;
            }

            let msg_size = write_buf.len() / msg_count.0 as usize;
            let mut offset = 0;
            for _ in 0..msg_count.0 {
                match FastMessage::parse(&write_buf[offset..offset+msg_size]) {
                    Ok(decoded_msg) => error_occurred = decoded_msg != msg,
                    Err(_) => error_occurred = true
                }
                offset += msg_size;
            }

            !error_occurred
        }
    }

    quickcheck! {
        fn prop_fast_message_decoding(msg: FastMessage, msg_count: MessageCount) -> bool {
            let mut write_buf = BytesMut::new();
            let mut error_occurred = false;
            let mut fast_msgs: Vec<FastMessage> =
                Vec::with_capacity(msg_count.0 as usize);

            (0..msg_count.0).for_each(|_| {
                fast_msgs.push(msg.clone());
            });

            let mut fast_rpc = FastRpc;
            let encode_res = fast_rpc.encode(fast_msgs, &mut write_buf);

            if encode_res.is_err() {
                return false;
            }

            let decode_result = fast_rpc.decode(&mut write_buf);
            if decode_result.is_err() {
                return false;
            }

            let m_decoded_msgs = decode_result.unwrap();


            if m_decoded_msgs.is_none() {
                return false;
            }

            let decoded_msgs = m_decoded_msgs.unwrap();
            if decoded_msgs.len() != msg_count.0 as usize {
                return false;
            }


            for decoded_msg in decoded_msgs {
                error_occurred = decoded_msg != msg;
            }

            !error_occurred
        }
    }
}
