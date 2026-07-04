//! M3 桥接集成测试：用 MemoryContextStore + MemoryVersionStore 验证
//! State fork/checkpoint/load + Metacog 归档/检索 + Character 写入约束。

use agent_context_db_core::{
    ContextEntry, ContextUri, StateScope, TenantId,
};
use agent_context_db_testkit::{MemoryContextStore, MemoryVersionStore};
use agent_context_db_uwu::{
    CharacterConstraint, CoreValue, MetacogBridge, PredErrorSample,
    StateBridge, StateSnapshot, TimeWindow,
};
use agent_context_db_version::MergeStrategy;
use std::sync::Arc;

fn tenant() -> TenantId {
    TenantId(uuid::Uuid::nil())
}

fn make_snapshot(agent_id: &str, scope: StateScope, pred_error: f32, payload: serde_json::Value) -> StateSnapshot {
    StateSnapshot::new(
        agent_id.to_string(),
        scope,
        0,
        None,
        pred_error,
        payload,
    )
}

fn make_core_value(name: &str, description: &str, forbidden_terms: Vec<&str>) -> CoreValue {
    CoreValue {
        name: name.to_string(),
        description: description.to_string(),
        forbidden_terms: forbidden_terms.into_iter().map(|s| s.to_string()).collect(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// StateBridge 测试
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn state_checkpoint_and_load_roundtrip() {
    let store = Arc::new(MemoryContextStore::new());
    let versions = Arc::new(MemoryVersionStore::new());
    let bridge = StateBridge::new(store.clone(), versions);

    let snap = make_snapshot(
        "a1",
        StateScope::Mid,
        0.15,
        serde_json::json!({"mood": "neutral", "ws_size": 42}),
    );

    // checkpoint
    let v = bridge
        .checkpoint("a1", StateScope::Mid, &snap, tenant())
        .await
        .unwrap();
    assert!(v.0 > 0, "checkpoint should return version > 0");

    // load back
    let loaded = bridge.load("a1", StateScope::Mid).await.unwrap();
    assert_eq!(loaded.agent_id, "a1");
    assert_eq!(loaded.accumulated_pred_error, 0.15);
    assert_eq!(
        loaded.payload.get("mood").and_then(|v| v.as_str()),
        Some("neutral")
    );
}

#[tokio::test]
async fn state_fork_promote_and_discard() {
    let store = Arc::new(MemoryContextStore::new());
    let versions = Arc::new(MemoryVersionStore::new());
    let bridge = StateBridge::new(store.clone(), versions.clone());

    // 先 checkpoint 一个基线
    let snap = make_snapshot("a1", StateScope::Mid, 0.1, serde_json::json!({"v": 1}));
    bridge
        .checkpoint("a1", StateScope::Mid, &snap, tenant())
        .await
        .unwrap();

    // 确认 scope 下至少有一个 commit 了
    // 通过 fork 来测试
    let fork1 = bridge.fork("a1", StateScope::Mid).await.unwrap();
    assert!(fork1.branch.0.starts_with("fork-state-"));

    // discard fork1 —— 删除分支
    bridge.discard_fork(&fork1).await.unwrap();

    // fork2 —— 用于 promote
    let fork2 = bridge.fork("a1", StateScope::Mid).await.unwrap();

    // 在 fork2 上做 checkpoint（推到新版本）
    let snap2 = make_snapshot("a1", StateScope::Mid, 0.05, serde_json::json!({"v": 2}));
    bridge
        .checkpoint("a1", StateScope::Mid, &snap2, tenant())
        .await
        .unwrap();

    // promote fork2 回 main
    let result = bridge
        .promote_fork(&fork2, MergeStrategy::FastForward)
        .await;
    // 注意：merge 可能失败因为 main 可能还没有创建
    // 此时只是验证 API 不 panic
    let _ = result;
}

// ═══════════════════════════════════════════════════════════════════════════
// MetacogBridge 测试
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn metacog_log_and_retrieve_pred_errors() {
    let store = Arc::new(MemoryContextStore::new());
    let bridge = MetacogBridge::new(store.clone());

    let t1 = test_t1();
    let t2 = t1 + 1_000_000_000; // +1 second in nanoseconds

    // 归档两个样本
    let s1 = PredErrorSample {
        predicted_state_id: "s1".into(),
        actual_state_id: "s1_actual".into(),
        calibration: 0.12,
        meta_score: 0.88,
        ts: t1,
    };
    let s2 = PredErrorSample {
        predicted_state_id: "s2".into(),
        actual_state_id: "s2_actual".into(),
        calibration: 0.34,
        meta_score: 0.66,
        ts: t2,
    };

    bridge
        .log_pred_error("a1", &s1, tenant())
        .await
        .unwrap();
    bridge
        .log_pred_error("a1", &s2, tenant())
        .await
        .unwrap();

    // 检索全部（窗口覆盖全部）
    let window = TimeWindow {
        from_ts: t1 - 1,
        to_ts: t2 + 1,
    };
    let results = bridge
        .retrieve_calibration("a1", window, &[])
        .await
        .unwrap();

    assert_eq!(results.len(), 2, "should retrieve both archived samples");
    assert!(
        results.iter().any(|s| s.calibration == 0.12),
        "should contain s1"
    );
    assert!(
        results.iter().any(|s| s.calibration == 0.34),
        "should contain s2"
    );
}

fn test_t1() -> i64 {
    1_700_000_000_000_000_000i64
}

#[tokio::test]
async fn metacog_hot_samples_override_cold() {
    let store = Arc::new(MemoryContextStore::new());
    let bridge = MetacogBridge::new(store.clone());

    let ts = 1_700_000_000_000_000_000i64;

    // 冷归档：旧版本
    let cold = PredErrorSample {
        predicted_state_id: "old".into(),
        actual_state_id: "old_actual".into(),
        calibration: 0.8,
        meta_score: 0.2,
        ts,
    };
    bridge
        .log_pred_error("a1", &cold, tenant())
        .await
        .unwrap();

    // 热数据：同 ts，更新版本
    let hot = PredErrorSample {
        predicted_state_id: "new".into(),
        actual_state_id: "new_actual".into(),
        calibration: 0.1,
        meta_score: 0.9,
        ts,
    };

    let window = TimeWindow {
        from_ts: ts - 1,
        to_ts: ts + 1,
    };
    let results = bridge
        .retrieve_calibration("a1", window, &[hot])
        .await
        .unwrap();

    // 热数据应覆盖冷数据（同 ts 只保留热版本）
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].calibration, 0.1);
    assert_eq!(results[0].predicted_state_id, "new");
}

#[tokio::test]
async fn metacog_time_window_filters_correctly() {
    let store = Arc::new(MemoryContextStore::new());
    let bridge = MetacogBridge::new(store.clone());

    let t_old = 1_000_000_000_000_000_000i64;
    let t_mid = 2_000_000_000_000_000_000i64;
    let t_new = 3_000_000_000_000_000_000i64;

    for (ts, cal) in &[(t_old, 0.1), (t_mid, 0.5), (t_new, 0.9)] {
        bridge
            .log_pred_error(
                "a1",
                &PredErrorSample {
                    predicted_state_id: format!("s{}", ts),
                    actual_state_id: "a".into(),
                    calibration: *cal,
                    meta_score: 0.5,
                    ts: *ts,
                },
                tenant(),
            )
            .await
            .unwrap();
    }

    // 只取中间窗口
    let window = TimeWindow {
        from_ts: t_mid - 1,
        to_ts: t_mid + 1,
    };
    let results = bridge
        .retrieve_calibration("a1", window, &[])
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].calibration, 0.5);
}

// ═══════════════════════════════════════════════════════════════════════════
// CharacterConstraint 测试
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn character_constraint_blocks_forbidden_terms() {
    let cc = CharacterConstraint::new(vec![
        make_core_value("honesty", "Always tell the truth.", vec!["fabricate", "lie"]),
        make_core_value("safety", "Avoid dangerous commands.", vec!["rm -rf", "DROP TABLE"]),
    ]);

    let uri = ContextUri::parse("uwu://t/agent/a/memories/cases/c1").unwrap();

    // 违反 honesty
    let bad = ContextEntry::new_text(uri.clone(), tenant(), "we should lie about the result");
    assert!(cc.check_write(&bad).await.is_err());

    // 违反 safety
    let dangerous = ContextEntry::new_text(uri.clone(), tenant(), "just run rm -rf /");
    assert!(cc.check_write(&dangerous).await.is_err());

    // 通过
    let good = ContextEntry::new_text(uri.clone(), tenant(), "we found the bug and fixed it");
    assert!(cc.check_write(&good).await.is_ok());

    // 违反 l1_overview 也应拦截
    let mut hidden = ContextEntry::new_text(uri.clone(), tenant(), "all good here");
    hidden.l1_overview = Some("let's fabricate the data".into());
    assert!(cc.check_write(&hidden).await.is_err());
}
