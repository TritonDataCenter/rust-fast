// Copyright 2019 Joyent, Inc.

//! Fast: A simple RPC protcol used by Joyent products
//!
//! Fast is a simple RPC protocol used in
//! Joyent's[Triton](http://github.com/joyent/triton) and
//! [Manta](https://github.com/joyent/manta) systems, particularly in the
//! [Moray](https://github.com/joyent/moray) and
//! [Boray](https://github.com/joyent/boray) components.
//!
//! Protocol overview
//!
//! The Fast protocol is intended for use with TCP.  Typically, a Fast server
//! listens for TCP connections on a well-known port, and Fast clients connect
//! to the server to make RPC requests.  Clients can make multiple connections
//! to the server, but each connection represents a logically separate
//! client. Communication between client and server consist of discrete
//! _messages_ sent over the TCP connection.
//!
//! Fast protocol messages have the following structure:
//!
//! <img src="../../../docs/fastpacket.svg" width="100%" height="100%">
//!
//! * VERSION   1-byte integer.  The only supported value is "1".
//!
//! * TYPE      1-byte integer.  The only supported value is TYPE_JSON (0x1),
//!           indicating that the data payload is an encoded JSON object.
//!
//! * STATUS    1-byte integer.  The only supported values are:
//!
//!     * STATUS_DATA  0x1  indicates a "data" message
//!
//!     * STATUS_END   0x2  indicates an "end" message
//!
//!     * STATUS_ERROR 0x3  indicates an "error" message
//!
//! * MSGID0...MSGID3    4-byte big-endian unsigned integer, a unique identifier
//!                    for this message.
//!
//! * CRC0...CRC3        4-byte big-endian unsigned integer representing the CRC16
//!                     value of the data payload
//!
//! * DLEN0...DLEN4      4-byte big-endian unsigned integer representing the number
//!                    of bytes of data payload that follow
//!
//! * DATA0...DATAN      Data payload.  This is a JSON-encoded object (for TYPE =
//!                    TYPE_JSON).  The encoding length in bytes is given by the
//!                    DLEN0...DLEN4 bytes.
//!
//! ### Status
//!
//! There are three allowed values for `status`:
//!
//! |Status value | Status name | Description |
//! |------------ | ----------- | ----------- |
//! | `0x1`        | `DATA`      | From clients, indicates an RPC request.  From servers, indicates one of many values emitted by an RPC call.|
//! | `0x2`        | `END`       | Indicates the successful completion of an RPC call.  Only sent by servers. |
//! | `0x3`        | `ERROR`     | Indicates the failed completion of an RPC call.  Only sent by servers. |
//!
//! ### Message IDs
//!
//! Each Fast message has a message id, which is scoped to the Fast
//! connection.  These are allocated sequentially from a circular 31-bit space.
//!
//! ### Data payload
//!
//! For all messages, the `data` field contains properties:
//!
//! | Field    | Type              | Purpose |
//! | -------- | ----------------- | ------- |
//! | `m`      | object            | describes the RPC method being invoked |
//! | `m.name` | string            | name of the RPC method being invoked |
//! | `m.uts`  | number (optional) | timestamp of message creation, in microseconds since the Unix epoch |
//! | `d`      | object or array   | varies by message status |
//!
//! ### Messaging Scenarios
//!
//! Essentially, there are only four messaging scenarios with Fast:
//!
//! **Client initiates an RPC request.** The client allocates a new message
//! identifier and sends a `DATA` message with `data.m.name` set to the name of
//! the RPC method it wants to invoke.  Arguments are specified by the array
//! `data.d`. Clients may issue concurrent requests over a single TCP
//! connection, provided they do not re-use a message identifier for separate
//! requests.
//!
//! **Server sends data from an RPC call.** RPC calls may emit an arbitrary
//! number of values back to the client.  To emit these values, the server sends
//! `DATA` messages with `data.d` set to an array of non-null values to be
//! emitted.  All `DATA` messages for the same RPC request have the same message
//! identifier that the client included in its original `DATA` message that
//! initiated the RPC call.
//!
//! **Server completes an RPC call successfully.** When an RPC call completes
//! successfully, the server sends an `END` event having the same message
//! identifier as the one in the client's original `DATA` message that initiated
//! the RPC call. This message can contain data as well, in which case it should
//! be processed the same way as for a DATA message.
//!
//! **Server reports a failed RPC call.** Any time before an `END` message is
//! generated for an RPC call, the server may send an `ERROR` message having the
//! same message identifier as the one in the client's original `DATA` message
//! that initiated the RPC call.
//!
//! By convention, the `m` fields (`m.name` and `m.uts`) are populated for all
//! server messages, even though `m.name` is redundant.
//!
//! The RPC request begins when the client sends the initial `DATA` message.
//! The RPC request is finished when the server sends either an `ERROR` or `END`
//! message for that request.  In summary, the client only ever sends one
//! message for each request.  The server may send any number of `DATA` messages
//! and exactly one `END` or `ERROR` message.

#![allow(missing_docs)]

pub mod client;
pub mod protocol;
pub mod server;
