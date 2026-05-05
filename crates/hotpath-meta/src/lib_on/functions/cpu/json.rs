use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct Profile {
    #[serde(default)]
    pub(crate) libs: Vec<Lib>,
    #[serde(default)]
    pub(crate) threads: Vec<Thread>,
}

#[derive(Deserialize, Default)]
pub(crate) struct Lib {
    #[serde(default)]
    pub(crate) path: Option<String>,
    #[serde(rename = "debugPath", default)]
    pub(crate) debug_path: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct Thread {
    pub(crate) samples: Samples,
    #[serde(rename = "stackTable")]
    pub(crate) stack_table: StackTable,
    #[serde(rename = "frameTable")]
    pub(crate) frame_table: FrameTable,
    #[serde(rename = "funcTable")]
    pub(crate) func_table: FuncTable,
    #[serde(rename = "resourceTable")]
    pub(crate) resource_table: ResourceTable,
}

#[derive(Deserialize)]
pub(crate) struct Samples {
    #[serde(default)]
    pub(crate) stack: Vec<Option<usize>>,
    #[serde(default)]
    pub(crate) weight: Option<Vec<i64>>,
    #[serde(rename = "threadCPUDelta", default)]
    pub(crate) thread_cpu_delta: Option<Vec<i64>>,
}

#[derive(Deserialize)]
pub(crate) struct StackTable {
    #[serde(default)]
    pub(crate) prefix: Vec<Option<usize>>,
    #[serde(default)]
    pub(crate) frame: Vec<usize>,
}

#[derive(Deserialize)]
pub(crate) struct FrameTable {
    #[serde(default)]
    pub(crate) address: Vec<i64>,
    #[serde(default)]
    pub(crate) func: Vec<usize>,
}

#[derive(Deserialize)]
pub(crate) struct FuncTable {
    #[serde(default)]
    pub(crate) resource: Vec<i64>,
}

#[derive(Deserialize)]
pub(crate) struct ResourceTable {
    #[serde(default)]
    pub(crate) lib: Vec<Option<i64>>,
}
