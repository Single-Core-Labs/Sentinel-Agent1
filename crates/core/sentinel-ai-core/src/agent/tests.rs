#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn registry_basic_flow() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let registry = AgentRegistry::new(2);
            // Register two agents successfully.
            let id1 = registry.register("gpt-4o").await.expect("first agent");
            let id2 = registry.register("gpt-4o-mini").await.expect("second agent");
            // Third registration should fail (capacity reached).
            assert!(matches!(registry.register("gpt-3.5").await, Err(RegistryError::CapacityReached(2))));
            // Retrieve an existing agent.
            let agent = registry.get(id1).await.expect("get agent");
            assert_eq!(agent.model, "gpt-4o");
            // Unregister one and add another.
            registry.unregister(id1).await.expect("unregister");
            let id3 = registry.register("gpt-3.5").await.expect("third after free slot");
            assert_eq!(registry.count().await, 2);
            // Ensure the new ID is reachable.
            let _ = registry.get(id3).await.expect("new agent");
        });
    }
}
