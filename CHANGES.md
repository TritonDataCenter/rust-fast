# Changelog

## Not yet released

Breaking change to the client API. The `send` function is changed so that
`msg_id` parameter is a `FastMessageId` value rather than a mutable
reference. This is to enable a scenario where a single `TcpStream` used for a
fast-rpc client needs to be shared among multiple threads. The mutable reference
prevents an associated `FastMessageId` from also being shared across threads and
since the `FastMessageId` is a newtype around an atomic value there is no need
for the mutable reference in the first place. A `Clone` instance is also added
for `FastMessageId`.

## 0.3.0

Change the package name to fast-rpc to avoid naming conflict when publishing to
crates.io.

## 0.2.0

Update fast protocol version to 2 to be compatible with node-fast version 3.0.0
servers.
