# Just configuration for mevlog-backend

# Default recipe
default:
    @just --list

# Start the server with asset timestamping and environment setup
test_all:
    cargo test --test hotpath_integration -- --nocapture --test-threads=1
    cargo test --test streams -- --nocapture --test-threads=1
    cargo test --test channels_crossbeam -- --nocapture --test-threads=1
    cargo test --test channels_futures -- --nocapture --test-threads=1
    cargo test --test channels_std -- --nocapture --test-threads=1
    cargo test --test channels_tokio -- --nocapture --test-threads=1
