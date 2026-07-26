# Live TUI Console Reference

Terminal-based TUI for real-time monitoring of profiling metrics. See `CLAUDE.md` for project guidance.

## Building the TUI

```bash
# Build the hotpath binary with TUI support
cargo build --bin hotpath --features tui
```

## Using the TUI (two-terminal workflow)

Terminal 1 - Start your profiled application (metrics server starts automatically):

```bash
# The metrics server starts by default on port 6770
cargo run -p test-tokio-async --example long_running --features hotpath

# Or customize the port
HOTPATH_METRICS_PORT=6770 cargo run -p test-tokio-async \
  --example long_running --features hotpath
```

Terminal 2 - Launch the TUI console:

```bash
# Connect to the metrics server
cargo run --bin hotpath --features tui -- console --metrics-port 6770

# Or with custom refresh interval (in milliseconds)
cargo run --bin hotpath --features tui -- console \
  --metrics-port 6770 --refresh-interval 1000
```

## Keyboard Controls

- `q/Q` - Quit the TUI
- `p/P` - Pause/resume automatic refresh
- `o/O` - Toggle logs panel (shows recent logs for selected function)
- `j/Down` - Navigate down in function list
- `k/Up` - Navigate up in function list

## Features

- Real-time display of profiling metrics
- Sortable function table with performance statistics
- Live logs panel for individual function monitoring
- **Channels monitoring** - View real-time channel statistics:
  - Messages sent/received counts
  - Current queue size, queued bytes, and max queue size
  - Channel state (active, closed, notified); fullness is derived from queue depth vs capacity, not a state
  - Channel type (bounded, unbounded, oneshot)
  - Message send/receive logs (when `log = true`)
- **Streams monitoring** - View real-time stream statistics:
  - Items yielded count
  - Stream state (active, closed)
  - Item yield logs (when `log = true`)
- **Futures monitoring** - View real-time future lifecycle:
  - Poll counts and completion status
  - Future state (active, completed, cancelled)
  - Optional output logging
- **RwLocks monitoring** - View real-time RwLock wait & acquire-time statistics (Data Flow sub-tab):
  - Split into two stacked tables, reads on top and writes below, sharing one selection cursor; each row shows count plus average and configured-percentile durations for wait time (blocked before the lock is granted) and acquire time (held duration, granted -> released)
  - Four histograms per lock: read-wait, read-acquire, write-wait, write-acquire
  - The terminal report mirrors this with two sub-tables under the single `rw_locks` section (write sub-table is skipped if there were no writes)
  - No per-event logs (table-only)
- **Mutexes monitoring** - View real-time Mutex wait & acquire-time statistics (Data Flow sub-tab):
  - A single table (no read/write split - a mutex has only one lock kind); each row shows lock count plus average and configured-percentile durations for wait time (blocked before the lock is granted) and acquire time (held duration, granted -> released)
  - Two histograms per lock: wait, acquire
  - The terminal report mirrors this with a single `mutexes` section
  - No per-event logs (table-only)
- **SQL monitoring** - View real-time SQL query execution time, on the **I/O** top-level tab (`[3]`), SQL sub-tab:
  - One row per *normalized* query (parameter-varied executions merge into one bucket), showing call count, average, and configured-percentile durations plus total
  - Requires `hotpath::sqlx_tracing_layer()` added to the app's `tracing` subscriber (`sqlx` feature)
  - The terminal report mirrors this with a single `sql` section; metrics also at `GET /sql`
  - Per-query execution logs panel; the panel border shows the query's `source` (innermost instrumented function at call time, tracked via `caller_stack.rs`)
- **HTTP monitoring** - View real-time HTTP request metrics, on the **I/O** top-level tab, HTTP sub-tab:
  - One row per normalized endpoint (`METHOD host/path` with `{id}` placeholders) with call count, error count, average and configured-percentile durations plus total
  - Requires wrapping the reqwest client with `hotpath::http!` (`reqwest-0-12`/`reqwest-0-13` feature)
  - Per-endpoint request logs panel with `source` attribution; metrics also at `GET /http`
- **I/O bytes monitoring** - View real-time byte-level I/O metrics (`io!` wrappers), on the **I/O** top-level tab, Bytes sub-tab:
  - Reads and writes as stacked sub-tables: operation count, bytes, per-operation transfer rate, average and configured-percentile durations, flush count (writes), errors
  - No per-operation logs panel (aggregate tables only); metrics also at `GET /io`
- **Threads monitoring** - View real-time thread CPU usage:
  - Per-thread CPU usage percentage (current and max)
  - Platform-specific collectors (Linux/macOS/Windows)
- **Tokio runtime monitoring** - View real-time Tokio runtime metrics:
  - Per-worker stats: park count, busy duration, poll count, steal count
  - Global stats: alive tasks, queue depths, blocking threads, IO driver metrics
  - Additional metrics available with `tokio_unstable` cfg flag
- Support for all profiling modes (time, alloc-bytes-total, alloc-count-total)
- Automatic data refresh with configurable interval
- Pause/resume functionality
- Error handling with connection status display
