use codex_core::agent::{AgentRegistry, RegistryError};
use tokio::runtime::Runtime;

#[test]
fn test_registry_basic_flow() {
    let rt = Runtime::new().expect("create rt");
    rt.block_on(async {
        let registry = AgentRegistry::new(2);
        // Register two agents.
        let id1 = registry.register("gpt-4o").await.expect("first");
        let id2 = registry.register("gpt-4o-mini").await.expect("second");
        // Third should hit capacity.
        match registry.register("gpt-3.5").await {
            Err(RegistryError::CapacityReached(2)) => {}
            _ => panic!("expected capacity error"),
        }
        // Retrieve and verify.
        let agent = registry.get(id1).await.expect("get");
        assert_eq!(agent.model, "gpt-4o");
        // Unregister and add a new one.
        registry.unregister(id1).await.expect("unregister");
        let id3 = registry.register("gpt-3.5").await.expect("new after free");
        assert_eq!(registry.count().await, 2);
        // Ensure new one exists.
        let _ = registry.get(id3).await.expect("new exists");
    });
}
