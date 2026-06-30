# Rust async monitoring: Futures, Streams and Channels performance

`hotpath` lets you monitor Rust channels performance in real time - alongside streams and futures. Track channel throughput, queue depth, identify slow consumers, monitor futures resolution, and discover bottlenecks while your system is running. With minimal instrumentation, you can get a clear picture of how data moves through your app's async pipeline.

All monitoring macros (`channel!`, `stream!`, `future!` and `future_fn`) are noop unless `hotpath` feature is activated.

## Channels monitoring

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

### Supported channel libraries

[std::sync](https://doc.rust-lang.org/stable/std/sync/mpsc/index.html) channels are instrumented by default. Enable the matching feature flag for each third-party library.

#### [std](https://github.com/rust-lang/rust)

Built-in, no feature flag required.

- [`std::sync::mpsc::channel`](https://doc.rust-lang.org/stable/std/sync/mpsc/fn.channel.html)
- [`std::sync::mpsc::sync_channel`](https://doc.rust-lang.org/stable/std/sync/mpsc/fn.sync_channel.html)

#### [Tokio](https://github.com/tokio-rs/tokio)

Enable the `tokio` feature.

- [`tokio::sync::mpsc::channel`](https://docs.rs/tokio/latest/tokio/sync/mpsc/fn.channel.html)
- [`tokio::sync::mpsc::unbounded_channel`](https://docs.rs/tokio/latest/tokio/sync/mpsc/fn.unbounded_channel.html)
- [`tokio::sync::oneshot::channel`](https://docs.rs/tokio/latest/tokio/sync/oneshot/fn.channel.html)

#### [futures-rs](https://github.com/rust-lang/futures-rs)

Enable the `futures` feature.

- [`futures_channel::mpsc::channel`](https://docs.rs/futures-channel/latest/futures_channel/mpsc/fn.channel.html)
- [`futures_channel::mpsc::unbounded`](https://docs.rs/futures-channel/latest/futures_channel/mpsc/fn.unbounded.html)
- [`futures_channel::oneshot::channel`](https://docs.rs/futures-channel/latest/futures_channel/oneshot/fn.channel.html)

#### [async-channel](https://github.com/smol-rs/async-channel)

Enable the `async-channel` feature.

- [`async_channel::bounded`](https://docs.rs/async-channel/latest/async_channel/fn.bounded.html)
- [`async_channel::unbounded`](https://docs.rs/async-channel/latest/async_channel/fn.unbounded.html)

#### [crossbeam](https://github.com/crossbeam-rs/crossbeam)

Enable the `crossbeam` feature.

- [`crossbeam_channel::bounded`](https://docs.rs/crossbeam/latest/crossbeam/channel/fn.bounded.html)
- [`crossbeam_channel::unbounded`](https://docs.rs/crossbeam/latest/crossbeam/channel/fn.unbounded.html)

#### [flume](https://github.com/zesterer/flume)

Enable the `flume` feature.

- [`flume::bounded`](https://docs.rs/flume/latest/flume/fn.bounded.html)
- [`flume::unbounded`](https://docs.rs/flume/latest/flume/fn.unbounded.html)


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

Tokio, crossbeam, and async-channel channels don't require this parameter because their capacity is accessible from the channel handles.

Bounded `std::sync::mpsc` channels wrapped with `wrap = true` also require `capacity`, and **the value must match the `sync_channel(N)` argument**:

```rust
use std::sync::mpsc;

// std bounded wrap - capacity MUST equal the sync_channel argument
let (tx, rx) = hotpath::channel!(mpsc::sync_channel::<String>(100), wrap = true, capacity = 100);
```

Wrap mode rebuilds the inner channel from `capacity` (std exposes no way to read it back from the endpoints) and discards the channel you constructed. If the two disagree - e.g. `sync_channel(100)` with `capacity = 1` - the profiled build gets a different bound than the unprofiled one (where `channel!` returns your original channel untouched), which can change backpressure or even deadlock only when profiling is enabled. Keep the numbers equal.

### A note on accuracy

`hotpath` instruments channels by using a proxy on the receive side with the capacity of 1. Messages flow directly into your original channel, then through a proxy before reaching the consumer. Sent/received counts are observed at the proxy boundary (between the original channel and the proxy), not at the final consumer. In practice, the observable results closely reflect the real ones - counts will match exactly once messages pass through the proxy. 

Please note that enabling monitoring can subtly affect channel behavior in some cases. For example, using `try_send` may behave slightly differently since the proxy adds 1 slot of extra capacity. Also some wrappers currently do not propagate info about receiver getting dropped.

I'm actively improving the library, so any feedback, issues, bug reports are appreciated.

### Send-receive latency and queue depth (`wrap = true`)

For `crossbeam`, `std`, `tokio` (`mpsc`), and `flume` channels you can opt into **endpoint wrapping** with `wrap = true`. Instead of inserting a forwarder-proxy, this wraps the `Sender`/`Receiver` directly and stamps each message with its send time, so the report gains an exact **send-receive latency** histogram (`proc_avg` plus the configured percentiles), alongside an exact live queue depth:

```rust
let (tx, rx) = hotpath::channel!(
    crossbeam_channel::unbounded::<i32>(),
    wrap = true,
    label = "jobs"
);

// tokio mpsc, bounded or unbounded - no `capacity` argument needed
let (tx, rx) = hotpath::channel!(
    tokio::sync::mpsc::channel::<i32>(100),
    wrap = true,
    label = "jobs"
);
```

The recorded latency is the full interval from `send()` to `recv()`, including backpressure wait on bounded channels. Because the timestamps are taken inside your own `send`/`recv` calls rather than in a forwarder task or thread, the value is exact - and wrap mode is also lighter than the proxy, since it adds no extra task/thread or hop. Tokio and flume benefit the most: their proxies relay every message through a background task and a second channel, costing a scheduler round-trip per message, whereas wrap mode hits the real channel directly.

Tokio bounded wrap channels do not need a `capacity` argument - the bound is recovered from `Sender::max_capacity()`. flume wrap channels (bounded or unbounded) likewise need no `capacity` argument - the bound is recovered from the endpoint.

Latency is reported **only for wrap channels**. A proxy channel stamps its events inside the forwarder thread, in the middle of the pipeline, so it cannot observe the producer-side or consumer-side wait accurately. Prefer `wrap = true` when you care about channel latency.

## Streams monitoring

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

## Futures monitoring

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
