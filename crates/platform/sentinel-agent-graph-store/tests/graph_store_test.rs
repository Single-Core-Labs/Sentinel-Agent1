use sentinel_agent_graph_store::graph::{AgentGraph, ThreadSpawnEdge, SpawnEdgeStatus, AgentNode};
use sentinel_agent_graph_store::store::{AgentGraphStore, GraphStoreError};
use sentinel_agent_graph_store::local::LocalAgentGraphStore;

// ---------------------------------------------------------------------------
// In-memory AgentGraph tests
// ---------------------------------------------------------------------------

#[test]
fn test_graph_new_is_empty() {
    let graph = AgentGraph::new();
    assert_eq!(graph.nodes().len(), 0);
    assert!(graph.root().is_empty());
}

#[test]
fn test_graph_add_edge_creates_child() {
    let mut graph = AgentGraph::new();
    let edge = ThreadSpawnEdge::new("parent-1", "child-1");
    graph.add_edge(edge);

    let children = graph.children("parent-1");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].child_thread_id, "child-1");
    assert!(children[0].is_open());
}

#[test]
fn test_graph_upsert_edge_creates_new() {
    let mut graph = AgentGraph::new();
    graph.upsert_edge("p1", "c1", SpawnEdgeStatus::Open);

    let children = graph.children("p1");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].child_thread_id, "c1");
}

#[test]
fn test_graph_upsert_edge_updates_existing() {
    let mut graph = AgentGraph::new();
    graph.upsert_edge("p1", "c1", SpawnEdgeStatus::Open);
    graph.upsert_edge("p1", "c1", SpawnEdgeStatus::Closed);

    let children = graph.children("p1");
    assert_eq!(children.len(), 1);
    assert!(children[0].is_closed());
}

#[test]
fn test_graph_set_edge_status_changes_status() {
    let mut graph = AgentGraph::new();
    graph.add_edge(ThreadSpawnEdge::new("p1", "c1"));
    graph.set_edge_status("p1", "c1", SpawnEdgeStatus::Closed);

    let children = graph.children("p1");
    assert!(children[0].is_closed());
}

#[test]
fn test_graph_set_edge_status_nonexistent_is_noop() {
    let mut graph = AgentGraph::new();
    graph.set_edge_status("p1", "c1", SpawnEdgeStatus::Closed);
}

#[test]
fn test_graph_children_ordered_by_created_at() {
    let mut graph = AgentGraph::new();
    graph.add_edge(ThreadSpawnEdge::new("p1", "c2"));
    std::thread::sleep(std::time::Duration::from_millis(5));
    graph.add_edge(ThreadSpawnEdge::new("p1", "c1"));

    let children = graph.children("p1");
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].child_thread_id, "c2");
    assert_eq!(children[1].child_thread_id, "c1");
}

#[test]
fn test_graph_descendants_traverses_recursively() {
    let mut graph = AgentGraph::new();
    graph.add_edge(ThreadSpawnEdge::new("root", "child-a"));
    graph.add_edge(ThreadSpawnEdge::new("child-a", "grandchild-1"));
    graph.add_edge(ThreadSpawnEdge::new("child-a", "grandchild-2"));
    graph.add_edge(ThreadSpawnEdge::new("root", "child-b"));

    let desc = graph.descendants("root");
    assert_eq!(desc.len(), 4);

    let desc_ids: Vec<&str> = desc.iter().map(|e| e.child_thread_id.as_str()).collect();
    assert!(desc_ids.contains(&"child-a"));
    assert!(desc_ids.contains(&"child-b"));
    assert!(desc_ids.contains(&"grandchild-1"));
    assert!(desc_ids.contains(&"grandchild-2"));
}

#[test]
fn test_graph_descendants_empty_for_leaf() {
    let mut graph = AgentGraph::new();
    graph.add_edge(ThreadSpawnEdge::new("p1", "c1"));
    let desc = graph.descendants("c1");
    assert!(desc.is_empty());
}

#[test]
fn test_graph_root_finds_edges_without_parents() {
    let mut graph = AgentGraph::new();
    graph.add_edge(ThreadSpawnEdge::new("root-a", "child-a"));
    graph.add_edge(ThreadSpawnEdge::new("child-a", "grandchild"));
    graph.add_edge(ThreadSpawnEdge::new("root-b", "child-b"));

    let roots = graph.root();
    assert_eq!(roots.len(), 2);
    let root_ids: Vec<&str> = roots.iter().map(|e| e.parent_thread_id.as_str()).collect();
    assert!(root_ids.contains(&"root-a"));
    assert!(root_ids.contains(&"root-b"));
}

#[test]
fn test_graph_all_nodes() {
    let mut graph = AgentGraph::new();
    graph.add_edge(ThreadSpawnEdge::new("root", "child"));
    graph.add_edge(ThreadSpawnEdge::new("child", "grandchild"));

    let nodes: Vec<AgentNode> = graph.nodes();
    assert_eq!(nodes.len(), 3);

    let root_node = nodes.iter().find(|n| n.thread_id == "root").unwrap();
    assert!(root_node.parent_id.is_none());

    let child_node = nodes.iter().find(|n| n.thread_id == "child").unwrap();
    assert_eq!(child_node.parent_id.as_deref(), Some("root"));

    let gc_node = nodes.iter().find(|n| n.thread_id == "grandchild").unwrap();
    assert_eq!(gc_node.parent_id.as_deref(), Some("child"));
}

#[test]
fn test_graph_clear_removes_all_edges() {
    let mut graph = AgentGraph::new();
    graph.add_edge(ThreadSpawnEdge::new("p1", "c1"));
    graph.add_edge(ThreadSpawnEdge::new("p1", "c2"));
    assert_eq!(graph.nodes().len(), 3);

    graph.clear();
    assert!(graph.nodes().is_empty());
    assert!(graph.root().is_empty());
}

#[test]
fn test_edge_new_creates_open_edge() {
    let edge = ThreadSpawnEdge::new("parent-1", "child-1");
    assert!(edge.is_open());
    assert!(!edge.is_closed());
    assert!(!edge.id.is_empty());
}

// ---------------------------------------------------------------------------
// LocalAgentGraphStore (SQLite) tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_local_store_in_memory_crud() {
    let store = LocalAgentGraphStore::in_memory().unwrap();

    store.upsert_thread_spawn_edge("root", "child-a", SpawnEdgeStatus::Open).await.unwrap();
    store.upsert_thread_spawn_edge("root", "child-b", SpawnEdgeStatus::Open).await.unwrap();

    let children = store.list_thread_spawn_children("root").await.unwrap();
    assert_eq!(children.len(), 2);

    let child_a = children.iter().find(|e| e.child_thread_id == "child-a").unwrap();
    assert_eq!(child_a.parent_thread_id, "root");
    assert!(child_a.is_open());
}

#[tokio::test]
async fn test_local_store_upsert_updates_status() {
    let store = LocalAgentGraphStore::in_memory().unwrap();

    store.upsert_thread_spawn_edge("p1", "c1", SpawnEdgeStatus::Open).await.unwrap();
    store.upsert_thread_spawn_edge("p1", "c1", SpawnEdgeStatus::Closed).await.unwrap();

    let children = store.list_thread_spawn_children("p1").await.unwrap();
    assert_eq!(children.len(), 1);
    assert!(children[0].is_closed());
}

#[tokio::test]
async fn test_local_store_set_status() {
    let store = LocalAgentGraphStore::in_memory().unwrap();

    store.upsert_thread_spawn_edge("p1", "c1", SpawnEdgeStatus::Open).await.unwrap();
    store.set_thread_spawn_edge_status("p1", "c1", SpawnEdgeStatus::Closed).await.unwrap();

    let children = store.list_thread_spawn_children("p1").await.unwrap();
    assert!(children[0].is_closed());
}

#[tokio::test]
async fn test_local_store_set_status_nonexistent_returns_error() {
    let store = LocalAgentGraphStore::in_memory().unwrap();

    let result = store.set_thread_spawn_edge_status("p1", "c1", SpawnEdgeStatus::Closed).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        GraphStoreError::EdgeNotFound { parent, child } => {
            assert_eq!(parent, "p1");
            assert_eq!(child, "c1");
        }
        other => panic!("Expected EdgeNotFound, got {:?}", other),
    }
}

#[tokio::test]
async fn test_local_store_get_parent() {
    let store = LocalAgentGraphStore::in_memory().unwrap();
    store.upsert_thread_spawn_edge("parent-1", "child-1", SpawnEdgeStatus::Open).await.unwrap();

    let parent = store.get_thread_parent("child-1").await.unwrap();
    assert!(parent.is_some());
    assert_eq!(parent.unwrap().parent_thread_id, "parent-1");
}

#[tokio::test]
async fn test_local_store_get_parent_nonexistent() {
    let store = LocalAgentGraphStore::in_memory().unwrap();
    let parent = store.get_thread_parent("orphan").await.unwrap();
    assert!(parent.is_none());
}

#[tokio::test]
async fn test_local_store_list_descendants() {
    let store = LocalAgentGraphStore::in_memory().unwrap();

    store.upsert_thread_spawn_edge("root", "child-a", SpawnEdgeStatus::Open).await.unwrap();
    store.upsert_thread_spawn_edge("child-a", "grandchild-1", SpawnEdgeStatus::Open).await.unwrap();
    store.upsert_thread_spawn_edge("child-a", "grandchild-2", SpawnEdgeStatus::Open).await.unwrap();

    let desc = store.list_thread_spawn_descendants("root").await.unwrap();
    assert_eq!(desc.len(), 3);
}

#[tokio::test]
async fn test_local_store_list_root_threads() {
    let store = LocalAgentGraphStore::in_memory().unwrap();

    store.upsert_thread_spawn_edge("root-a", "child-a", SpawnEdgeStatus::Open).await.unwrap();
    store.upsert_thread_spawn_edge("root-b", "child-b", SpawnEdgeStatus::Open).await.unwrap();
    store.upsert_thread_spawn_edge("child-a", "grandchild", SpawnEdgeStatus::Open).await.unwrap();

    let roots = store.list_root_threads().await.unwrap();
    assert_eq!(roots.len(), 2);
    let root_parents: Vec<&str> = roots.iter().map(|e| e.parent_thread_id.as_str()).collect();
    assert!(root_parents.contains(&"root-a"));
    assert!(root_parents.contains(&"root-b"));
}

#[tokio::test]
async fn test_local_store_list_all_nodes() {
    let store = LocalAgentGraphStore::in_memory().unwrap();

    store.upsert_thread_spawn_edge("root", "child", SpawnEdgeStatus::Open).await.unwrap();

    let nodes = store.list_all_nodes().await.unwrap();
    assert_eq!(nodes.len(), 2);

    let root_node = nodes.iter().find(|n| n.thread_id == "root").unwrap();
    assert!(root_node.parent_id.is_none(), "root should have no parent");

    let child_node = nodes.iter().find(|n| n.thread_id == "child").unwrap();
    assert_eq!(child_node.parent_id.as_deref(), Some("root"));
}

#[tokio::test]
async fn test_local_store_clear() {
    let store = LocalAgentGraphStore::in_memory().unwrap();

    store.upsert_thread_spawn_edge("p1", "c1", SpawnEdgeStatus::Open).await.unwrap();
    store.clear().await.unwrap();

    let nodes = store.list_all_nodes().await.unwrap();
    assert!(nodes.is_empty());
}

#[tokio::test]
async fn test_local_store_file_persistence() {
    let dir = std::env::temp_dir().join(format!("graph-store-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    let db_str = db_path.to_str().unwrap();

    {
        let store = LocalAgentGraphStore::open(db_str).unwrap();
        store.upsert_thread_spawn_edge("root", "child", SpawnEdgeStatus::Open).await.unwrap();
    }

    {
        let store = LocalAgentGraphStore::open(db_str).unwrap();
        let children = store.list_thread_spawn_children("root").await.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].child_thread_id, "child");

        let nodes = store.list_all_nodes().await.unwrap();
        assert_eq!(nodes.len(), 2);
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn test_local_store_listing_returns_deterministic_order() {
    let store = LocalAgentGraphStore::in_memory().unwrap();

    store.upsert_thread_spawn_edge("root", "c", SpawnEdgeStatus::Open).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    store.upsert_thread_spawn_edge("root", "a", SpawnEdgeStatus::Open).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    store.upsert_thread_spawn_edge("root", "b", SpawnEdgeStatus::Open).await.unwrap();

    let children = store.list_thread_spawn_children("root").await.unwrap();
    assert_eq!(children[0].child_thread_id, "c");
    assert_eq!(children[1].child_thread_id, "a");
    assert_eq!(children[2].child_thread_id, "b");
}

#[tokio::test]
async fn test_local_store_sqlite_error_on_invalid_path() {
    let invalid_path = "/nonexistent/deeply/nested/db.sqlite";
    let result = LocalAgentGraphStore::open(invalid_path);
    assert!(result.is_err());
}

#[test]
fn test_graph_store_error_display() {
    let err = GraphStoreError::DatabaseError("disk full".into());
    assert!(err.to_string().contains("disk full"));

    let err = GraphStoreError::EdgeNotFound { parent: "p".into(), child: "c".into() };
    let msg = err.to_string();
    assert!(msg.contains("p"));
    assert!(msg.contains("c"));
}
