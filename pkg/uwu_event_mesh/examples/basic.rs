//! Demo: typed event sets, causal envelopes, persistence, replay,
//! cross-mesh federation via `ChannelBridge`, and segmented persistence.
//!
//! Run with: `cargo run --example basic`

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uwu_event_mesh::prelude::*;

#[derive(Debug, Serialize, Deserialize)]
struct OrderCreated {
    id: u64,
    amount: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct OrderShipped {
    id: u64,
    tracking_no: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // 1. Mesh backed by a JSON-Lines on-disk store.
    let log_path = std::env::temp_dir().join("uwu_event_mesh_demo.jsonl");
    let idx_path = log_path.with_extension("jsonl.idx");
    let _ = std::fs::remove_file(&log_path);
    let _ = std::fs::remove_file(&idx_path);
    let store = Arc::new(JsonlStore::open(&log_path).await.unwrap());
    let mesh = EventMesh::with_store(store.clone());

    // 2. Typed event set under namespace `flow.order`.
    let order_set = EventSet::new(&mesh, "flow.order").unwrap();
    let created_kind = order_set.kind::<OrderCreated>("created");

    let mut sub_created = order_set.subscribe(&created_kind);
    let mut sub_all = mesh.subscribe_str("flow.>").unwrap();

    // 3. Producer task: emit a created + a causally-linked shipped.
    let producer_set = EventSet::new(&mesh, "flow.order").unwrap();
    let producer_created = producer_set.kind::<OrderCreated>("created");
    let producer_shipped = producer_set.kind::<OrderShipped>("shipped");
    let producer = tokio::spawn(async move {
        producer_set
            .emit(&producer_created, &OrderCreated { id: 1, amount: 99.0 })
            .await
            .unwrap();

        let parent_topic = Topic::new("flow.order.created").unwrap();
        let parent = Envelope::new(&parent_topic, serde_json::json!({"id": 1}))
            .with_source("demo-producer");
        let payload = serde_json::to_value(OrderShipped {
            id: 1,
            tracking_no: "SF123456".into(),
        })
        .unwrap();
        let child = Envelope::child_of(&parent, &producer_shipped.topic(), payload)
            .with_source("demo-producer");
        producer_set.publish(&producer_shipped, child).await.unwrap();
    });

    // 4. Read typed `created`.
    if let Some(Ok((_env, payload))) = sub_created.recv().await {
        println!(
            "[typed] OrderCreated: id={} amount={}",
            payload.id, payload.amount
        );
    }

    // 5. Read raw envelopes (Arc<Envelope>) from wildcard.
    for _ in 0..2 {
        if let Some(env) = sub_all.recv().await {
            println!(
                "[raw] topic={} root={} parent={:?} payload={}",
                env.topic, env.root_id, env.parent_id, env.payload
            );
        }
    }
    producer.await.unwrap();

    // 6. Flush + replay from disk into a fresh subscriber.
    store.flush().await.unwrap();
    let mut replay_sub = mesh.subscribe_str("flow.>").unwrap();
    let history = mesh
        .replay(ReplayFilter::topic("flow.>").unwrap(), true)
        .await
        .unwrap();
    println!("\n[replay] {} historical events:", history.len());
    for _ in 0..history.len() {
        if let Some(env) = replay_sub.recv().await {
            println!("  - replayed topic={} id={}", env.topic, env.id);
        }
    }

    // 7. Cross-mesh federation via ChannelBridge.
    let mesh_b = EventMesh::new();
    let pair = ChannelBridgePair::new();
    mesh.attach_bridge(pair.a_to_b.clone());
    mesh_b.attach_bridge(pair.b_to_a.clone());
    let mesh_b_pump = mesh_b.clone();
    let mut b_inbox = pair.b_inbox;
    let pump = tokio::spawn(async move {
        while let Some(env) = b_inbox.recv().await {
            let _ = mesh_b_pump.ingest_remote(env).await;
        }
    });
    let mut sub_b = mesh_b.subscribe_str("flow.>").unwrap();
    let t = Topic::new("flow.bridge.ping").unwrap();
    mesh.emit(&t, serde_json::json!({"hello": "remote"}))
        .await
        .unwrap();
    if let Some(env) = sub_b.recv().await {
        println!(
            "\n[bridge] mesh_b received topic={} payload={}",
            env.topic, env.payload
        );
    }

    // 8. Graceful shutdown: drains pending writes + fsync the log + idx.
    mesh.shutdown().await.unwrap();
    mesh_b.shutdown().await.unwrap();
    drop(pair.a_to_b);
    drop(pair.b_to_a);
    let _ = tokio::time::timeout(std::time::Duration::from_millis(200), pump).await;
    println!("[shutdown] flushed and stopped writer task");

    let _ = std::fs::remove_file(&log_path);
    let _ = std::fs::remove_file(&idx_path);
}
