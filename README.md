# rust-fast: streaming JSON RPC over TCP

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

## Caveats

Due to the fact that `node-fast` uses a buggy version of a CRC library the CRC
checking is currently disabled. Right now for the server to interact with the node Fast
client you must comment out the CRC checking in the client as well. The code to
be commented out is [here](https://github.com/joyent/node-fast/blob/35a89eeba56f557c3d018b6d2734f39a453c760c/lib/fast_protocol.js#L190-L200).

This was just a quick exploratory effort. As it stands now much of the code
could be more nicely factored and there is currently no logging. There are cases
where a program could crash unnecessarily. If you try the example server and it
crashes run it with `RUST_BACKTRACE=1 cargo run --example fastserve` to figure
out where the badness happened. The error handling also is very basic.
