# Rust async data flow monitoring: channels, streams, and futures

`hotpath` lets you observe async data flow in real time - across Rust channels, streams, and futures. You can analyze channel queues, slow consumers, and bottlenecks as they happen. It's designed for debugging live systems, helping you understand data flow bottlenecks. With minimal code changes, you get visibility into how data moves through app's async pipeline.

All monitoring macros are noop unless `hotpath` feature is activated.

## Channels

### channel! macro

This macro wraps channel creation to automatically track performance metrics and data flow:

```rust
use tokio::sync::mpsc;

#[tokio::main]
#[hotpath::main]
async fn main() {
    // Create and instrument a channel in one step
    let (tx, mut rx) = hotpath::channel!(mpsc::channel::<String>(100));

    // Use the channel exactly as before
    tx.send("Hello".to_string()).await.unwrap();
    let msg = rx.recv().await.unwrap();
}
```

[std::sync](https://doc.rust-lang.org/stable/std/sync/mpsc/index.html) channels can be instrumented by default. Enable `tokio`, `futures`, or `crossbeam` features for [Tokio](https://github.com/tokio-rs/tokio), [futures-rs](https://github.com/rust-lang/futures-rs), and [crossbeam](https://github.com/crossbeam-rs/crossbeam) channels, respectively.

**Supported channel types:**
- [`tokio::sync::mpsc::channel`](https://docs.rs/tokio/latest/tokio/sync/mpsc/fn.channel.html)
- [`tokio::sync::mpsc::unbounded_channel`](https://docs.rs/tokio/latest/tokio/sync/mpsc/fn.unbounded_channel.html)
- [`tokio::sync::oneshot::channel`](https://docs.rs/tokio/latest/tokio/sync/oneshot/fn.channel.html)
- [`futures_channel::mpsc::channel`](https://docs.rs/futures-channel/latest/futures_channel/mpsc/fn.channel.html)
- [`futures_channel::mpsc::unbounded`](https://docs.rs/futures-channel/latest/futures_channel/mpsc/fn.unbounded.html)
- [`futures_channel::oneshot::channel`](https://docs.rs/futures-channel/latest/futures_channel/oneshot/fn.channel.html)
- [`crossbeam_channel::bounded`](https://docs.rs/crossbeam/latest/crossbeam/channel/fn.bounded.html)
- [`crossbeam_channel::unbounded`](https://docs.rs/crossbeam/latest/crossbeam/channel/fn.unbounded.html)


### Optional config

```rust
// Custom label for easier identification in TUI
let (tx, rx) = hotpath::channel!(mpsc::channel::<String>(100), label = "worker_queue");

// Enable message logging (requires std::fmt::Debug trait on message type)
let (tx, rx) = hotpath::channel!(mpsc::channel::<String>(100), log = true);
```

Label channels to display them on top of the list. By passing `log = true` TUI will display messages that a channel received.

<img loading="lazy" src="{{#asset-hash images/channels-log.png}}" alt="hotpath-rs TUI showing channel message flow monitoring with send and receive logs">

### Capacity parameter requirement

For `futures::channel::mpsc` bounded channels, you **must** specify the `capacity` parameter because their API doesn't expose the capacity after creation:

```rust
use futures_channel::mpsc;

// futures bounded channel - MUST specify capacity
let (tx, rx) = hotpath::channel!(mpsc::channel::<String>(10), capacity = 10);
```

Tokio and crossbeam channels don't require this parameter because their capacity is accessible from the channel handles.

### A note on accuracy

`hotpath` instruments channels by using a proxy on the receive side with the capacity of 1. Messages flow directly into your original channel, then through a proxy before reaching the consumer. This design adds 1 slot of extra buffering for bounded channels.

Please note that enabling monitoring can subtly affect channel behavior in some cases. For example, using `try_send` may behave slightly differently since the proxy adds 1 slot of extra capacity. Also some wrappers currently not propagate info about receiver getting dropped.

I'm actively improving the library, so any feedback, issues, bug reports are appreciated.

## Streams

### stream! macro

This macro instruments async streams to track performance metrics and items yielded:

```rust
use futures::stream::{self, StreamExt};

#[tokio::main]
#[hotpath::main]
async fn main() {
    // Create and instrument a stream in one step
    let s = hotpath::stream!(stream::iter(1..=100));

    // Use it normally
    let items: Vec<_> = s.collect().await;
}
```

### Optional config

```rust
// Custom label
let s = hotpath::stream!(stream::iter(1..=100), label = "data_stream");

// Enable item logging (requires std::fmt::Debug trait on item type)
let s = hotpath::stream!(stream::iter(1..=100), log = true);
```

Label streams to display them on top of the list. By passing `log = true` TUI will display values that a stream yielded.

<img loading="lazy" src="{{#asset-hash images/streams-log.png}}" alt="hotpath-rs TUI showing async stream item monitoring and throughput">

## Futures

### future! and future_fn macros

The `future!` macro and `#[future_fn]` attribute instrument any async function or piece of code or to track poll counts and future lifecycle:

```rust
#[tokio::main]
#[hotpath::main]
async fn main() {
    // Instrument a future expression
    let result = hotpath::future!(async { 42 }, log = true).await;

    instrumented_fetch().await;
}

// Or use the attribute on async functions
#[hotpath::future_fn(log = true)]
async fn instrumented_fetch() -> Vec<u8> {
    vec![1, 2, 3]
}
```

### Optional config

```rust
// Custom label for easier identification in TUI
let result = hotpath::future!(async { 42 }, label = "my_future").await;

// Enable output logging (requires std::fmt::Debug trait on output type)
let result = hotpath::future!(async { 42 }, log = true).await;
```

Label futures to display them on top of the list. By passing `log = true` TUI will display values that future resolved to:

<img loading="lazy" src="{{#asset-hash images/futures-log.png}}" alt="hotpath-rs TUI showing async futures poll tracking and value logging">
