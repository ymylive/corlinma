//! Test 5 (stretch) — `cross_subsystem_wire_up`.
//!
//! Construct a `HookBus`, a `PlaceholderEngine`, and a loaded `Config` side by
//! side. Subscribe a handler to `HookEvent::ConfigChanged`, emit the event,
//! and assert the handler fires and carries the expected payload. The
//! engine + config are just along for the ride — the point is to prove the
//! Batch 1 primitives can be instantiated together in the same runtime
//! without ordering surprises.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use corlinman_core::config::Config;
use corlinman_core::placeholder::{
    DynamicResolver, PlaceholderCtx, PlaceholderEngine, PlaceholderError,
};
use corlinman_hooks::{HookBus, HookEvent, HookPriority};
use tokio::time::timeout;

struct StaticResolver(&'static str);

#[async_trait]
impl DynamicResolver for StaticResolver {
    async fn resolve(&self, _key: &str, _ctx: &PlaceholderCtx) -> Result<String, PlaceholderError> {
        Ok(self.0.to_string())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_subsystem_wire_up_fires_config_changed() {
    // --- Build all three subsystems. Sized per HooksConfig default. ---
    let cfg = Config::default();
    assert_eq!(cfg.hooks.capacity, 1024);
    let bus = HookBus::new(cfg.hooks.capacity);

    let mut engine = PlaceholderEngine::new();
    engine.register_namespace("session", Arc::new(StaticResolver("bob")));
    // Sanity: engine still works while the bus is alive.
    let ctx = PlaceholderCtx::new("sess-xs");
    let rendered = engine
        .render("user={{session.any}}", &ctx)
        .await
        .expect("engine render");
    assert_eq!(rendered, "user=bob");

    // --- Subscribe a handler on Normal priority.
    let mut handler = bus.subscribe(HookPriority::Normal);

    // --- Emit ConfigChanged from a different task to exercise the async path.
    let bus_for_emit = bus.clone();
    let emit_task = tokio::spawn(async move {
        bus_for_emit
            .emit(HookEvent::ConfigChanged {
                section: "hooks".into(),
                old: serde_json::json!({ "capacity": 1024 }),
                new: serde_json::json!({ "capacity": 2048 }),
            })
            .await
            .expect("emit ConfigChanged");
    });

    // --- Assert the handler fires with the expected payload.
    let event = timeout(Duration::from_secs(2), handler.recv())
        .await
        .expect("handler must receive within 2s")
        .expect("handler channel open");

    match event {
        HookEvent::ConfigChanged { section, old, new } => {
            assert_eq!(section, "hooks");
            assert_eq!(old, serde_json::json!({ "capacity": 1024 }));
            assert_eq!(new, serde_json::json!({ "capacity": 2048 }));
        }
        other => panic!("expected ConfigChanged, got {other:?}"),
    }

    emit_task.await.expect("emit task joined");
}
