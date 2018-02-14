use std::time::Duration;
use rand;
use serde_json;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SpanId(pub u64);

impl SpanId {
    pub fn new() -> Self {
        SpanId(rand::random())
    }
}

#[derive(Debug)]
pub enum AsyncOutcome {
    Success,
    Cancelled,
    Error(String),
}

#[derive(Debug)]
pub enum TraceEvent {
    AsyncStart {
        name: String,
        id: SpanId,
        parent_id: SpanId,
        ts: Duration,
        metadata: serde_json::Value,
    },
    AsyncOnCPU {
        id: SpanId,
        ts: Duration,
    },
    AsyncOffCPU {
        id: SpanId,
        ts: Duration,
    },
    AsyncEnd {
        id: SpanId,
        ts: Duration,
        outcome: AsyncOutcome,
    },

    SyncStart {
        name: String,
        id: SpanId,
        parent_id: SpanId,
        ts: Duration,
        metadata: serde_json::Value,
    },
    SyncEnd {
        id: SpanId,
        ts: Duration,
    },

    ThreadStart {
        name: String,
        id: SpanId,
        ts: Duration,
    },
    ThreadEnd {
        id: SpanId,
        ts: Duration,
    },

    Wakeup {
        waking_span: SpanId,
        parked_span: SpanId,
        ts: Duration,
    },
}
