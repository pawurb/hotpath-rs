# Changelog

All notable changes to this project will be documented in this file.

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
