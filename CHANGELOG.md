# Changelog

All notable changes to this project will be documented in this file.

## [0.20.0] - 2026-07-05

### 🚀 Features

- Add async_channel wrap version

- [**breaking**] Channel macro default wrap mode

Change the default channel instrumentation mode to wrap = true.
The previous default mode is now available via proxy = true config.


- Improve SQL TUI view

- Add time sampling config


### 🐛 Bug Fixes

- Use mutex drain functions by default and add hotpath-lockless

- Placeholders to fix lifecycle ordering issue


### ⚡ Performance

- Use wrap channels for meta instrumentation

- Add hotpath-lockless to all resource types

- Use SPSC for all resource metrics


### ⚙️ Miscellaneous Tasks

- Unify channel benchmarks

- Unify locks benchmarks

- Unify timing and alloc benchmarks

- Release 0.20.0


## [0.19.1] - 2026-06-30

### 🚀 Features

- Add tokio wrap channel support

- Add flume wrap channel support


### 🐛 Bug Fixes

- Ignore unused_braces warnings

- Update rmcp


### ⚙️ Miscellaneous Tasks

- Release 0.19.1


## [0.19.0] - 2026-06-29

### 🚀 Features

- Add initial SQL sqlx instrumentation

- Add diesel sql instrumentation


### 🚜 Refactor

- Rename sql_tracing_layer


### ⚙️ Miscellaneous Tasks

- Store repo adoption data

- Release 0.19.0


## [0.18.0] - 2026-06-22

### 🚀 Features

- Add mutexes_limit to macro

- Initial wrap crossbeam channel macro

- Add meta instrumentation to all worker channels

- [**breaking**] Add perf histogram to wrap channels

Stop displaying misleading log delay for non wrap channels


- Add little law example and adjust channel report

- [**breaking**] Add msg per s channel metric

Remove state and queue from channels table report.


- Add wrap support for std channels


### 🐛 Bug Fixes

- Misc fixes

- Measure_all compile time err

- Channels and futures set terminal states

- Fix queue_size race and add cargo hack CI

- Derive queue size

- Add message IDs for wrap channels

- Functions worker FIFO queue bug

- Channel macro params order

- Lib off channel macro params order

- Clamp queue size metric

- Channel rate per s calculation


### 🚜 Refactor

- Use unified enum for background workers

- Minor cleanup

- Simplify noop macros


### ⚙️ Miscellaneous Tasks

- Add benchmark_load.rs

- Release 0.18.0


## [0.17.0] - 2026-06-14

### 🚀 Features

- Initial hotpath::rw_lock! implementation [#340]

- Use histogram for RwLocks timing

- Instrument parking_lot::RwLock

- Add RwLocks to TUI

- Add async-lock RwLock support [#340]

- Track wait and acquire time for RwLocks

- Add tokio RwLock support

- Initial hotpath::mutex! implementation

- Add hotpath::mutex! support for Tokio

- Add hotpath::mutex! support for async-lock

- Add locks docs


### 🐛 Bug Fixes

- Add missing feature

- Add deprecated hotpath::wrap::new method

- Test crates features config


### ⚡ Performance

- Dont clone mutex and rw_locks stats_tx

- Profile after inner guard drop

- Add event batching to all resources

- Use custom Instant implementation on macos


### ⚙️ Miscellaneous Tasks

- Mutex/RwLock meta instrumentation and benchmarks

- Add channels instrumentation benchmarks

- Add futures and streams instrumentation benchmark

- Cleanup benchmark docs

- Add hotpath-alloc-meta to add test crates

- Benchmark_noop black_box

- Track Cargo lock file

- Cargo update CI check

- Add benchmarks spin wait

- Update ratatui

- Release 0.17.0


## [0.16.1] - 2026-05-17

### 🚀 Features

- Windows thread monitoring support

- Support flume channels


### ⚙️ Miscellaneous Tasks

- Release 0.16.1


## [0.16.0] - 2026-05-07

### 🚀 Features

- Group timing and memory functions tab

- [**breaking**] Separate tabs and endpoints for dataflow

Removes data_flow* json endpoints.


- Display avg total poll time

- Display PID in TUI status

- Initial CPU profiling

- Report cpu profiling errors

- CPU error handling and samply load

- Apply inline(never) to cpu measured functions

- Disable samply load command

- Abstract allocator in hotpath-alloc

- Render feature unavailable reason


### 🐛 Bug Fixes

- Report missing samples error

- Attribute cpu for labeled functions

- Calculate CPU for short profiling

- Feature-gated tests


### 🚜 Refactor

- [**breaking**] Remove ArcSwap for functions guard

Dropping functions guard will no longer reset its state.


- Improve deps versions and add CI

- Separate IDs for each dataflow type

- Reuse env helper


### ⚡ Performance

- Optional logging crates


### ⚙️ Miscellaneous Tasks

- Samply bin artifact CI

- Samply macOS bin artifact CI

- Add linux dev docs

- CI linux cpu profiling

- Add syncmeta skill

- Add cpu sampling docs

- Release 0.16.0


## [0.15.1] - 2026-04-27

### 🚀 Features

- Support label for functions

- Add HOTPATH_FUNCTIONS_NAME_DEPTH config


### 🚜 Refactor

- Unify batch measurements logic

- Remove unneeded aliases

- Minor cleanup


### ⚡ Performance

- Unify alloc and timing duration logic

- Send measurements in batches


### ⚙️ Miscellaneous Tasks

- Add cargo-publish task

- Update CI to rust 1.95

- Add samply docs

- Release 0.15.1


## [0.15.0] - 2026-04-09

### 🚀 Features

- Add configurable macro limits [#298]

- [**breaking**] Support float percentiles [#301]

- [**breaking**] Adjust HotpathGuardBuilder API

Remove `with_` prefix for all setters.


- Add per report ENV limit config

- [**breaking**] Exclusive alloc tracking by default, remove HOTPATH_ALLOC_SELF


### 🐛 Bug Fixes

- Fix duplicate meta allocator bug


### ⚡ Performance

- Make writing gauges and values faster


### ⚙️ Miscellaneous Tasks

- Update output_path docs

- Release 0.15.0


## [0.14.1] - 2026-03-27

### 🚀 Features

- Print total alloc metric

- Add missing MCP tools

- Compare avg thread CPU diff

- TUI UI fixes

- Add last value of debug/metrics to static report (#295)


### 🐛 Bug Fixes

- Fix total_allocated value for hotpath-alloc mode

- Report metrics port busy error [#286]

- Remove unused function


### ⚡ Performance

- Use futures channel ref


### ⚙️ Miscellaneous Tasks

- Release 0.14.1


## [0.14.0] - 2026-03-08

### 🚀 Features

- Display TUI version

- [**breaking**] Add HOTPATH_ALLOC_METRIC config

JSON schema rename: The `hotpath_profiling_mode` field in `JsonFunctionsList` is renamed to `profiling_mode`, and the `ProfilingMode::Alloc` variant is split into `AllocBytes` and `AllocCount` (serialized as "alloc-bytes" / "alloc-count" instead of "alloc"), breaking deserialization of existing JSON reports.


- Add UNSAFE_ASYNC_ALLOC

- Instrument future polls duration

- [**breaking**] Simplify functions data pipeline

Removed MetricsProvider trait and from public API.


- Add HOTPATH_MAX_LOG_LEN config

- Add future instrumentation to measure macro

- [**breaking**] Add runtime aware async alloc metrics

`MeasurementGuard`/`MeasurementGuardWithLog` replaced with `MeasurementGuardSync`/`MeasurementGuardSyncWithLog`/`MeasurementGuardAsync`/`MeasurementGuardAsyncWithLog`. Removed `HOTPATH_UNSAFE_ASYNC_ALLOC`.


- Improve TUI futures details

- Add async-channel support

- [**breaking**] Show avg thread CPU, remove sys and user time

- Configurable auto select index


### 🐛 Bug Fixes

- [**breaking**] Dont expose internal API and remove hotpath-off flag

- Add events drain limit on shutdown

- Add functions drain limit on shutdown

- [**breaking**] Dont expose HotpathGuard new

- Dont track instrumentation allocations

- Use raai guard for alloc tracking control

- Avoid closure lifetime macros bug

- Add nesting safety for SuspendAllocTracking

- Exclude channel events alloc tracking

- Remove unused custom waker

- Consistent data flow order

- Lower default HOTPATH_THREADS_INTERVAL_MS

- More deterministic self benchmarks

- Exclude hotpath threads alloc tracking

- Dont panic busy port

- Exclude hp-cpu-baseline thread alloc

- [**breaking**] Remove invalid channels queue depth reporting


### 🚜 Refactor

- Pass log to FutureEvent::Completed

- Unify Instant import across project

- [**breaking**] Cleanup macro guard builders logic

Refactor measurement macro/runtime naming and branching for consistency, including simplified measure_impl logic.


- Simplify send_future_event


### ⚡ Performance

- Prealloc VecDeque

- Optimize data flow fetch

- Optimize timing guard clock

- Dont log result unless future is visible

- Instrument only visible futures

- Cache alloc thread slots


### ⚙️ Miscellaneous Tasks

- Add alloc measure example and test

- Describe breaking changes in changelog [#247]

- Add all_noop example

- Instrument format_debug_truncated method

- Separate meta bench scripts

- Add test for all guard types

- Instrument is_focus function

- Dont build hotpath-utils for compare task

- Update docs

- Update benchmark_alloc

- Adjust alloc benchmark

- Release 0.14.0


## [0.13.0] - 2026-02-25

### 🚀 Features

- Rename config vars


### 🐛 Bug Fixes

- Remove unused feature flag

- Set default docs.rs flag

- Dont expose internal methods

- Fix default docs.rs flags

- Redefine mcp helper methods

- Fix docs and CI

- Fix outdated docs and invalid trait


### ⚙️ Miscellaneous Tasks

- Update docs

- Release 0.13.0


## [0.12.0] - 2026-02-23

### 🚀 Features

- Add G gg navigation

- Add JsonReport and parsing methods

- Remove raw json values

- Add hotpath-ci compare feature

- Rename hotpath-ci to hotpath-utils

- Integrate compare script with hotpath-utils

- Display diff labels

- Display thread metrics diff

- Show threads total alloc diff

- Add table header colors

- Add colors to compare report


### 🐛 Bug Fixes

- Publish meta crates

- Adjust tui profiling limit

- Shorten diff report functions names

- Improve diff report display

- Disable async alloc tracking in current_thread runtime

- Fix inflated alloc total

- Alloc measurement for cross thread guard

- Test_alloc_total_bytes_not_inflated test

- Fix profile-pr for new reports format

- Compare uniq threads by name

- Threads diff render

- Improve report desc


### 🚜 Refactor

- Cleanup compare metric tests


### ⚡ Performance

- Prebuild backend regexp


### ⚙️ Miscellaneous Tasks

- Test inline(always)

- Sync meta crate

- Add bench_docs script

- Sync hotpath-meta

- Release 0.12.0


## [0.11.0] - 2026-02-18

### 🚀 Features

- Default to hotpath console cmd

- Enable custom channels instrumentation [#163]

- Initial hotpath-meta integration

- Display profiled program uptime

- Add timeout to other guard types

- Support configurable guard timeouts

- Add threads guard

- Use unified reporting guard

- Support labels for future! macro

- Add HOTPATH_TUI_TAB for profiling support

- Show channels max queue

- Add before_report callback

- Configurable limits per section

- Show max CPU

- Add HOTPATH_FOCUS config

- Add bench script

- Add HOTPATH_TUI_REFRESH_INTERVAL_MS config

- Add HOTPATH_BENCH_RELEASE for bench scripts

- Add HOTPATH_SHUTDOWN_MS macro support

- Show cpu baseline


### 🐛 Bug Fixes

- Display tokio metrics hint

- Improve tokio metrics hint

- Sorted thread metrics

- Unify logs env config

- Fix meta build

- Init race condition crash

- Flaky tests

- Sitemap.xml lastmod

- Pass args to default cmd

- Unify static reports format

- Fix ssl CI error

- Use CPU time for baseline and sample results


### 🚜 Refactor

- Use ids for functions lookup

- Rename timeout to shutdown


### ⚡ Performance

- Fix channels instrumentation memory bloat

- Fix futures instrumentation memory bloat

- Fix streams instrumentation memory bloat

- Optimize debug logs memory

- Cache static ENV values

- U32 IDs and less string allocs

- Optimize data flow logs fetch

- Batch RwLock writes

- Prioritize function data queries and shutdown

- Cache functions query tx

- Cache alloc calculations


### ⚙️ Miscellaneous Tasks

- Add hotpath-meta to workspace

- Expand meta instrumentation

- Instrument meta hotpaths

- Sync meta crate

- Use unpublished meta crates

- Add CONTRIBUTING.md

- Sync meta crate

- Change default static report

- Release 0.11.0


## [0.10.1] - 2026-02-08

### 🚀 Features

- Tokio runtime metrics


### 🐛 Bug Fixes

- Fix TUI hightlight styling [#161]


### 🚜 Refactor

- TUI styling cleanup


### ⚙️ Miscellaneous Tasks

- Release 0.10.1


## [0.10.0] - 2026-02-02

### 🚀 Features

- Support nonlocal metrics host

- Scaffold MCP implementation

- Initial mcp tools

- Add mcp auth middleware

- Server side sorting

- More MCP tools and dev logging

- Customize MCP functions output

- MCP tools description

- Add MCP tools with params

- Improve MCP function logs output

- Use formatted JSON types for MCP

- Show total alloc dealloc stats

- Add initial debug info

- Initial crashtest TUI app

- Unify data flow TUI tab

- Add debug gauge! macro

- Support excluding wrappers

- Add file output_path support

- Support HOTPATH_OUTPUT_FORMAT config

- Add TUI table descriptions

- Support output silencing


### 🐛 Bug Fixes

- Val! macro improvements

- Improve TUI UI

- Silence stdout

- Silence dbg! warning

- Add missing noop traits and types

- Fix guards completion race condition

- Fix tui demo build


### 🚜 Refactor

- Rename and cleanup http

- Unify JSON formats

- Rearrange json modules

- Rearrange formatted traits

- Unify futures naming

- Simplify functions guard


### ⚡ Performance

- Use ids for data flow indexing


### ⚙️ Miscellaneous Tasks

- Annotate CLI tests

- Instrument tui events channel


## [0.9.3] - 2026-01-13

### 🚀 Features

- Add TUI logging


### 🐛 Bug Fixes

- Enable feature on docs.rs

- Use single proxy channel with minimal capacity


### 🚜 Refactor

- Dry cleanup and fix crash


### ⚡ Performance

- Async http requests and custom runtime

- Abort redundant http calls


### ⚙️ Miscellaneous Tasks

- Release 0.9.3


## [0.9.2] - 2025-12-22

### 🚀 Features

- Allow disabling http server


### 🐛 Bug Fixes

- Use localhost for metrics server [#109] (#110)


### ⚙️ Miscellaneous Tasks

- Update generated with link

- Update docs

- Release 0.9.2


## [0.9.1] - 2025-12-18

### 🚀 Features

- Add --benchmark-id config to hotpath-ci


### 🐛 Bug Fixes

- Display multi-byte characters [#104]


### ⚡ Performance

- Cache & batch function measurements (#107)


### ⚙️ Miscellaneous Tasks

- Add profiling examples

- Add more granular benchmarks

- Add macos benchmarks

- Release 0.9.1


## [0.9.0] - 2025-12-11

### 🚀 Features

- Add must_use for guards


### 🐛 Bug Fixes

- Fix hotpath CI integration

- Fix nested measure and improve auto-instrumentation

- Fix build on windows target [#93]

- Fix futures channel cancellation check

- Fix build warnings


### 🚜 Refactor

- Unify naming and structure


### ⚙️ Miscellaneous Tasks

- Improve auto-instrumentation demo

- Remove unneeded windows collector

- Remove unneeded hotpath feature dependency

- Release 0.9.0


## [0.8.0] - 2025-12-04

### 🚀 Features

- Lib noop as nonoptional dependency


### 🚜 Refactor

- Update readme, initial cleanup


### ⚙️ Miscellaneous Tasks

- Release 0.8.0


## [0.7.6] - 2025-12-02

### 🚀 Features

- Add futures instrumentation

- Display results of measured functions


### 🐛 Bug Fixes

- Use release profile for benchmark


### 🚜 Refactor

- Rename module

- Rename futures channels to ftc


### ⚡ Performance

- Measure alloc mode overhead separately


### ⚙️ Miscellaneous Tasks

- Release 0.7.6


## [0.7.5] - 2025-11-27

### 🚀 Features

- Display threads status info


### ⚡ Performance

- Always use quanta::Instant on linux


### ⚙️ Miscellaneous Tasks

- Release 0.7.5


## [0.7.4] - 2025-11-27

### 🚀 Features

- Display function position index

- Display per thread alloc dealloc stats


### 🐛 Bug Fixes

- Cleanup bottom bar

- Dont display cross thread exec TID


### ⚙️ Miscellaneous Tasks

- Release 0.7.4


## [0.7.3] - 2025-11-25

### 🚀 Features

- Initial threads monitoring


### 🐛 Bug Fixes

- Remove stream unsafe code with pin-project-lite

- Relax tokyo dependency

- Fix missing init panic message [#73]


### 🚜 Refactor

- Improve http routes logic


### ⚡ Performance

- Dont sleep in crossbeam channel wrapper


### ⚙️ Miscellaneous Tasks

- Release 0.7.3


## [0.7.2] - 2025-11-24

### 🚀 Features

- Auto-instrumentation demo


### 🐛 Bug Fixes

- Consistent sort order

- Fix handling for cross thread metrics

- Fix display for unsupported alloc metrics


### ⚡ Performance

- Auto instrumentation for hotpath TUI


### ⚙️ Miscellaneous Tasks

- Remove time profiling from CI

- Update mach2 dependency

- Release 0.7.2


## [0.7.1] - 2025-11-23

### 🚀 Features

- Show TID for function logs

- Support both timing and alloc metrics in TUI


### 🐛 Bug Fixes

- Always initialize START_TIME

- Exclude profiling overhead from alloc metrics

- Fix fetching correct function logs and index logic


### 🚜 Refactor

- Improve TUI UI

- Reuse TUI styles


### ⚙️ Miscellaneous Tasks

- Improve http endpoints tests

- Release 0.7.1


## [0.7.0] - 2025-11-22

### 🚀 Features

- Merge channels-console crate

- Show channels data in TUI

- Show streams data in TUI

- Add StreamsGuard, rearrange modules

- Unify alloc feature flags


### 🐛 Bug Fixes

- Improve memory metric display


### ⚙️ Miscellaneous Tasks

- Restore endpoint tests, add justfile

- Release 0.7.0


## [0.6.0] - 2025-11-15

### 🚀 Features

- Add live TUI interface (#50)

- Display time elapsed for samples

- Replace hotpath-alloc-self with HOTPATH_ALLOC_SELF

- Replace hotpath-ci with HOTPATH_JSON


### 🐛 Bug Fixes

- Fix build errors and warnings

- Display formatted bytes for alloc_bytes_total mode


### ⚙️ Miscellaneous Tasks

- Change default port value

- Release 0.6.0


## [0.5.3] - 2025-10-29

### 🚀 Features

- Hotpath guard Send + Sync, add build_with_timeout

- Add timeout macro param


### 🐛 Bug Fixes

- Use unbounded channel, upscale benchmark

- Increase time clamp range


### 🚜 Refactor

- Use named module file

- Remove unused guard, add alloc panic test


### ⚡ Performance

- Use Cell for alloc metrics


### ⚙️ Miscellaneous Tasks

- Release v0.5.3


## [0.5.2] - 2025-10-20

### 🚀 Features

- Add hotpath-alloc-self feature flag


### ⚙️ Miscellaneous Tasks

- Configure hotpath-macros dependency

- Adjust hotpath CI

- More secure hotpath CI setup

- Release v0.5.2


## [0.5.1] - 2025-10-19

### 🐛 Bug Fixes

- Support measure_all with all-features config


### ⚙️ Miscellaneous Tasks

- Release v0.5.1


## [0.5.0] - 2025-10-18

### 🚀 Features

- Add measure_all macro

- Add configurable limit and bugfixes

- Add hotpath::skip macro


### 🚜 Refactor

- Simplify measurement guards logic

- Use static str for caller_name

- Unify guards build logic


### ⚡ Performance

- Dont yield in benchmark example

- Use quanta on linux platforms


### ⚙️ Miscellaneous Tasks

- Use benchmark example for hotpath CI

- Release v0.5.0


## [0.4.1] - 2025-10-06

### 🚀 Features

- Add emoji to primary timing diff

- Dont spam CI comments


### ⚙️ Miscellaneous Tasks

- Release v0.4.1


## [0.4.0] - 2025-10-05

### 🚀 Features

- Add wrapper logic for outer functions

- Improve table display format


### 🐛 Bug Fixes

- Remove max allocation modes


### ⚙️ Miscellaneous Tasks

- Release v0.4.0


## [0.3.1] - 2025-10-05

### 🚀 Features

- Use emojis for outliers

- Add measurement guard to main macro


### 🐛 Bug Fixes

- Fix GitHub emojis and CI config


### ⚙️ Miscellaneous Tasks

- Use multiple Rust versions in CI

- Add unit test POC

- Release v0.3.1


## [0.3.0] - 2025-10-02

### 🚀 Features

- Implement custom reporting

- Add HotPathBuilder API

- Add Deserialize for MetricsJson

- Add Debug and Clone traits

- Add hotpath CLI for GitHub CI integration


### 🐛 Bug Fixes

- Fix MetricType serialization

- Improve MetricsJson deserializer

- Fix hotpath CLI config


### 🚜 Refactor

- Remove unused cfg_if

- Change metrics data structure, add JSON serializer

- Rename HotpathBuilder to GuardBuilder

- Rename MetricType


### ⚙️ Miscellaneous Tasks

- Test no op measure_block

- Add docs, reduce pub exports

- Improve docs, further reduce pub exports

- Release v0.3.0


## [0.2.10] - 2025-09-25

### 🐛 Bug Fixes

- Support --all-features config [#16]


### ⚙️ Miscellaneous Tasks

- Add test crates, improve alloc testing

- Release v0.2.10


## [0.2.9] - 2025-09-18

### 🐛 Bug Fixes

- Include tokio only for alloc features

- Fix measure_block cfg_if import [#13]


### ⚙️ Miscellaneous Tasks

- Release v0.2.9


## [0.2.8] - 2025-09-17

### 🐛 Bug Fixes

- Fix macro dependencies [#13][#2]


### ⚙️ Miscellaneous Tasks

- Release v0.2.8


## [0.2.6] - 2025-09-16

### 🚀 Features

- Support multiple reports per compilation [#2]


### 🐛 Bug Fixes

- Include tokio dependency [#13]


### ⚙️ Miscellaneous Tasks

- Cleanup deps and imports

- Release v0.2.6


## [0.2.5] - 2025-09-15

### 🚀 Features

- Add json output


### 🐛 Bug Fixes

- Relax dependencies versions

- Use edition 2021


### ⚙️ Miscellaneous Tasks

- Release v0.2.5


## [0.2.4] - 2025-09-13

### 🚀 Features

- Use p0 p100 instead of min max

- Noop measure blocks

- Make noop block the default

- Implement memory allocations tracking


### 🐛 Bug Fixes

- Reduce deps, exclude Cargo.lock


### ⚡ Performance

- Reduce Measurement size and add basic benchmark


### ⚙️ Miscellaneous Tasks

- Configure changelog

- Release 0.2.4


## [0.2.3] - 2025-09-08

<!-- generated by git-cliff -->
