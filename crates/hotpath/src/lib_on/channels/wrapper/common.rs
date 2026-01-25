//! Common utilities for channel wrappers.

use std::mem;

use crossbeam_channel::Sender as CbSender;

use crate::channels::{init_channels_state, ChannelEvent, ChannelType};
use crate::data_flow::next_data_flow_id;

#[cfg(target_os = "linux")]
pub use quanta::Instant;
#[cfg(not(target_os = "linux"))]
pub use std::time::Instant;

pub struct RegisteredChannel {
    pub id: u64,
    pub stats_tx: CbSender<ChannelEvent>,
}

pub fn register_channel<T>(
    source: &'static str,
    label: Option<String>,
    channel_type: ChannelType,
) -> RegisteredChannel {
    let type_name = std::any::type_name::<T>();
    let (stats_tx, _) = init_channels_state();
    let id = next_data_flow_id();

    let _ = stats_tx.send(ChannelEvent::Created {
        id,
        source,
        display_label: label,
        channel_type,
        type_name,
        type_size: mem::size_of::<T>(),
    });

    RegisteredChannel {
        id,
        stats_tx: stats_tx.clone(),
    }
}
