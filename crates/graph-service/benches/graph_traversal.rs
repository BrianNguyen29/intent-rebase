//! Graph traversal benchmarks — in-memory repository
//!
//! Scope: Benchmarks graph traversal operations with in-memory repository.
//! - bench_bfs_reachable: BFS reachability across small/medium/large graphs
//! - bench_find_path: Shortest path finding across small/medium/large graphs
//! - bench_cycle_detection: Cycle detection across small/medium/large graphs
//!
//! Not covered (future scope):
//! - SQL-backed graph repository benchmarks (requires live DB)
//! - Concurrent graph operations
//! - Graph classification/impact analysis benchmarks

use criterion::{criterion_group, criterion_main, Criterion};
use graph_service::{GraphService, InMemoryGraphRepository};
use intent_rebase_types::{
    CreateGraphEdgeRequest, CreateGraphNodeRequest, EdgeType, GraphNodeFilter, NodeType,
    TraversalOptions,
};
use uuid::Uuid;

/// Create a test node for benchmarking
fn create_node(tenant_id: Uuid, workflow_id: Uuid, node_type: NodeType) -> CreateGraphNodeRequest {
    let label = format!("{:?}-node", node_type);
    CreateGraphNodeRequest {
        tenant_id,
        workflow_id,
        node_type,
        external_ref: None,
        label,
        properties: None,
    }
}

/// Setup a small chain graph: a -> b -> c -> d
async fn setup_small_chain(
    graph: &GraphService,
    tenant_id: Uuid,
    workflow_id: Uuid,
) -> (Uuid, Uuid, Uuid, Uuid) {
    let a = graph
        .add_node(create_node(tenant_id, workflow_id, NodeType::Intent))
        .await
        .unwrap();
    let b = graph
        .add_node(create_node(tenant_id, workflow_id, NodeType::IntentVersion))
        .await
        .unwrap();
    let c = graph
        .add_node(create_node(tenant_id, workflow_id, NodeType::Artifact))
        .await
        .unwrap();
    let d = graph
        .add_node(create_node(tenant_id, workflow_id, NodeType::Approval))
        .await
        .unwrap();

    graph
        .add_edge(CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a.id,
            to_node_id: b.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        })
        .await
        .unwrap();

    graph
        .add_edge(CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: b.id,
            to_node_id: c.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        })
        .await
        .unwrap();

    graph
        .add_edge(CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: c.id,
            to_node_id: d.id,
            edge_type: EdgeType::ValidatedBy,
            properties: None,
        })
        .await
        .unwrap();

    (a.id, b.id, c.id, d.id)
}

/// Setup a medium star graph: center connected to 20 leaves
async fn setup_medium_star(graph: &GraphService, tenant_id: Uuid, workflow_id: Uuid) -> Uuid {
    let center = graph
        .add_node(create_node(tenant_id, workflow_id, NodeType::IntentVersion))
        .await
        .unwrap();

    for i in 0..20 {
        let leaf = graph
            .add_node(CreateGraphNodeRequest {
                tenant_id,
                workflow_id,
                node_type: NodeType::Artifact,
                external_ref: None,
                label: format!("leaf-{}", i),
                properties: None,
            })
            .await
            .unwrap();

        graph
            .add_edge(CreateGraphEdgeRequest {
                tenant_id,
                workflow_id,
                from_node_id: center.id,
                to_node_id: leaf.id,
                edge_type: EdgeType::DependsOn,
                properties: None,
            })
            .await
            .unwrap();
    }

    center.id
}

/// Setup a large tree graph: 5 levels, branching factor 3
async fn setup_large_tree(graph: &GraphService, tenant_id: Uuid, workflow_id: Uuid) -> Uuid {
    // Build a tree with 3^0 + 3^1 + 3^2 + 3^3 + 3^4 = 1 + 3 + 9 + 27 + 81 = 121 nodes
    let root = graph
        .add_node(create_node(tenant_id, workflow_id, NodeType::Intent))
        .await
        .unwrap();

    let mut level_nodes = vec![root.id];

    for level in 1..5 {
        let mut next_level = Vec::new();
        for &parent_id in &level_nodes {
            for i in 0..3 {
                let child = graph
                    .add_node(CreateGraphNodeRequest {
                        tenant_id,
                        workflow_id,
                        node_type: if level == 4 {
                            NodeType::Artifact
                        } else {
                            NodeType::IntentVersion
                        },
                        external_ref: None,
                        label: format!("level{}-child{}-{}", level, parent_id, i),
                        properties: None,
                    })
                    .await
                    .unwrap();

                graph
                    .add_edge(CreateGraphEdgeRequest {
                        tenant_id,
                        workflow_id,
                        from_node_id: parent_id,
                        to_node_id: child.id,
                        edge_type: EdgeType::DependsOn,
                        properties: None,
                    })
                    .await
                    .unwrap();

                next_level.push(child.id);
            }
        }
        level_nodes = next_level;
    }

    root.id
}

/// Benchmark BFS reachability across graph sizes
fn bench_bfs_reachable(c: &mut Criterion) {
    // Build graphs once outside the benchmark loop
    let (graph_small, start_small) = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let graph = GraphService::new(std::sync::Arc::new(InMemoryGraphRepository::new()));
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let (_, _, _, d) = setup_small_chain(&graph, tenant_id, workflow_id).await;
        (graph, d)
    });

    let (graph_medium, start_medium) = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let graph = GraphService::new(std::sync::Arc::new(InMemoryGraphRepository::new()));
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let start = setup_medium_star(&graph, tenant_id, workflow_id).await;
        (graph, start)
    });

    let (graph_large, start_large) = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let graph = GraphService::new(std::sync::Arc::new(InMemoryGraphRepository::new()));
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let start = setup_large_tree(&graph, tenant_id, workflow_id).await;
        (graph, start)
    });

    let mut group = c.benchmark_group("bfs_reachable");

    group.bench_with_input("small_4_nodes", &start_small, |b, &start| {
        let graph = graph_small.clone();
        b.iter(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let _ = graph
                    .find_reachable(
                        start,
                        TraversalOptions {
                            max_depth: Some(10),
                            include_start: false,
                            ..Default::default()
                        },
                    )
                    .await;
            });
        });
    });

    group.bench_with_input("medium_21_nodes", &start_medium, |b, &start| {
        let graph = graph_medium.clone();
        b.iter(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let _ = graph
                    .find_reachable(
                        start,
                        TraversalOptions {
                            max_depth: Some(10),
                            include_start: false,
                            ..Default::default()
                        },
                    )
                    .await;
            });
        });
    });

    group.bench_with_input("large_121_nodes", &start_large, |b, &start| {
        let graph = graph_large.clone();
        b.iter(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let _ = graph
                    .find_reachable(
                        start,
                        TraversalOptions {
                            max_depth: Some(10),
                            include_start: false,
                            ..Default::default()
                        },
                    )
                    .await;
            });
        });
    });

    group.finish();
}

/// Benchmark path finding across graph sizes
fn bench_find_path(c: &mut Criterion) {
    let (graph_small, a_small, d_small) = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let graph = GraphService::new(std::sync::Arc::new(InMemoryGraphRepository::new()));
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let (a, _, _, d) = setup_small_chain(&graph, tenant_id, workflow_id).await;
        (graph, a, d)
    });

    let (graph_medium, center_medium, leaf_medium) =
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let graph = GraphService::new(std::sync::Arc::new(InMemoryGraphRepository::new()));
            let tenant_id = Uuid::new_v4();
            let workflow_id = Uuid::new_v4();
            let center = setup_medium_star(&graph, tenant_id, workflow_id).await;

            let leaves: Vec<_> = graph
                .list_nodes(GraphNodeFilter {
                    tenant_id: Some(tenant_id),
                    workflow_id: Some(workflow_id),
                    node_type: Some(NodeType::Artifact),
                    state: None,
                })
                .await
                .unwrap();

            (graph, center, leaves.first().unwrap().id)
        });

    let (graph_large, root_large, leaf_large) =
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let graph = GraphService::new(std::sync::Arc::new(InMemoryGraphRepository::new()));
            let tenant_id = Uuid::new_v4();
            let workflow_id = Uuid::new_v4();
            let root = setup_large_tree(&graph, tenant_id, workflow_id).await;

            let leaves: Vec<_> = graph
                .list_nodes(GraphNodeFilter {
                    tenant_id: Some(tenant_id),
                    workflow_id: Some(workflow_id),
                    node_type: Some(NodeType::Artifact),
                    state: None,
                })
                .await
                .unwrap();

            (graph, root, leaves.first().unwrap().id)
        });

    let mut group = c.benchmark_group("find_path");

    group.bench_with_input("small_4_nodes", &(a_small, d_small), |b, &(src, dst)| {
        let graph = graph_small.clone();
        b.iter(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let _ = graph
                    .find_path(
                        src,
                        dst,
                        TraversalOptions {
                            max_depth: Some(10),
                            include_start: false,
                            ..Default::default()
                        },
                    )
                    .await;
            });
        });
    });

    group.bench_with_input(
        "medium_21_nodes",
        &(center_medium, leaf_medium),
        |b, &(src, dst)| {
            let graph = graph_medium.clone();
            b.iter(|| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let _ = graph
                        .find_path(
                            src,
                            dst,
                            TraversalOptions {
                                max_depth: Some(10),
                                include_start: false,
                                ..Default::default()
                            },
                        )
                        .await;
                });
            });
        },
    );

    group.bench_with_input(
        "large_121_nodes",
        &(root_large, leaf_large),
        |b, &(src, dst)| {
            let graph = graph_large.clone();
            b.iter(|| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let _ = graph
                        .find_path(
                            src,
                            dst,
                            TraversalOptions {
                                max_depth: Some(10),
                                include_start: false,
                                ..Default::default()
                            },
                        )
                        .await;
                });
            });
        },
    );

    group.finish();
}

/// Benchmark cycle detection across graph sizes
fn bench_cycle_detection(c: &mut Criterion) {
    let (graph_small, workflow_small) = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let graph = GraphService::new(std::sync::Arc::new(InMemoryGraphRepository::new()));
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        setup_small_chain(&graph, tenant_id, workflow_id).await;
        (graph, workflow_id)
    });

    let (graph_medium, workflow_medium) = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let graph = GraphService::new(std::sync::Arc::new(InMemoryGraphRepository::new()));
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        setup_medium_star(&graph, tenant_id, workflow_id).await;
        (graph, workflow_id)
    });

    let (graph_large, workflow_large) = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let graph = GraphService::new(std::sync::Arc::new(InMemoryGraphRepository::new()));
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        setup_large_tree(&graph, tenant_id, workflow_id).await;
        (graph, workflow_id)
    });

    let mut group = c.benchmark_group("cycle_detection");

    group.bench_with_input("small_4_nodes", &workflow_small, |b, &workflow_id| {
        let graph = graph_small.clone();
        b.iter(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let _ = graph.detect_cycles(workflow_id).await;
            });
        });
    });

    group.bench_with_input("medium_21_nodes", &workflow_medium, |b, &workflow_id| {
        let graph = graph_medium.clone();
        b.iter(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let _ = graph.detect_cycles(workflow_id).await;
            });
        });
    });

    group.bench_with_input("large_121_nodes", &workflow_large, |b, &workflow_id| {
        let graph = graph_large.clone();
        b.iter(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let _ = graph.detect_cycles(workflow_id).await;
            });
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_bfs_reachable,
    bench_find_path,
    bench_cycle_detection,
);
criterion_main!(benches);
