// Copyright 2019 Joyent, Inc.

//! Fast: A simple RPC protcol used by Joyent products
//!
//! Fast is a simple RPC protocol used in
//! Joyent's[Triton](http://github.com/joyent/triton) and
//! [Manta](https://github.com/joyent/manta) systems, particularly in the
//! [Moray](https://github.com/joyent/moray) and
//! [Boray](https://github.com/joyent/boray) components.
//!
//! Protocol definition
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
//! * MSGID1...MSGID4    4-byte big-endian unsigned integer, a unique identifier
//!                    for this message
//!
//! * CRC1...CRC4        4-byte big-endian unsigned integer representing the CRC16
//!                     value of the data payload
//!
//! * DLEN0...DLEN4      4-byte big-endian unsigned integer representing the number
//!                    of bytes of data payload that follow
//!
//! * DATA0...DATAN      Data payload.  This is a JSON-encoded object (for TYPE =
//!                    TYPE_JSON).  The encoding length in bytes is given by the
//!                    DLEN0...DLEN4 bytes.
//!
//! Message IDs: each Fast message has a message id, which is scoped to the Fast
//! connection.  These are allocated sequentially from a circular 31-bit space.

#![allow(missing_docs)]

pub mod client;
pub mod protocol;
pub mod server;
