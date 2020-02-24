# fast-rpc: streaming JSON RPC over TCP

Fast is a simple RPC protocol used in Joyent's
[Triton](http://github.com/joyent/triton) and
[Manta](https://github.com/joyent/manta) systems, particularly in the
[Moray](https://github.com/joyent/moray) key-value store.  This README contains
usage notes.

The original implementation is
[node-fast](https://github.com/joyent/node-fast). This is the rust
implementation of the Fast protocol.

This crate includes:

* client library interface
* server library interface
* `fastserve`, An example Fast server for demo and testing
* `fastcall`, An example command-line tool for making Fast RPC requests

## Synopsis

Start the rust Fast server:

    $ cargo run --example fastserve

Use the `fastcall` example program to invoke the `date` RPC method inside the
client:

```
cargo run --example fastcall -- --args '[]' --method date

```

The `fastcall` program in the [`node-fast`](https://github.com/joyent/node-fast)
repo will also work:

    $ fastcall 127.0.0.1 2030 date '[]'
    {"timestamp":1457475515355,"iso8601":"2016-03-08T22:18:35.355Z"}

Or try the `yes` method, an RPC version of yes(1):

    $ fastcall 127.0.0.1 2030 yes '[ { "value": { "hello": "world" }, "count": 3 } ]'
    {"hello":"world"}
    {"hello":"world"}
    {"hello":"world"}

## Documentation

Further information is available in the rustdocs. These can be generated locally
by running `cargo doc` in the cloned repository.
