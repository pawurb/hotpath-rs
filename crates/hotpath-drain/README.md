# hotpath-drain

Lock-free per-thread event queues with a single drain, extracted from the [hotpath](https://github.com/pawurb/hotpath-rs) profiler.

Each producer thread owns a chunked SPSC queue. A single consumer sweeps every registered queue, so events buffered on parked threads still reach the drain.

Work in progress. The API is not stable yet.
