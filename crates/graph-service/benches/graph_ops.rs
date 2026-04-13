//! Graph operations benchmarks for graph-service
//!
//! This module measures the latency of graph traversal operations
//! (reachability, path-finding, cycle detection) using criterion.
//!
//! Note: This is a benchmark harness only — it verifies the benchmark
//! infrastructure builds and runs. Actual performance targets and production
//! load testing remain outstanding (gated on P5 — Phase 3 Batch 4a completion).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use graph_service::{GraphService, InMemoryGraphRepository};
use intent_rebase_types::{
    CreateGraphEdgeRequest, CreateGraphNodeRequest, EdgeType, ExternalRef, ExternalRefType,
    NodeType, TraversalOptions,
};
use std::sync::Arc;
use tokio::runtime::Runtime;
use uuid::Uuid;

/// Helper: create a test node request with fixed tenant/workflow for edge wiring
fn make_node_request(
    tenant_id: Uuid,
    workflow_id: Uuid,
    node_type: NodeType,
    label: &str,
) -> CreateGraphNodeRequest {
    CreateGraphNodeRequest {
        tenant_id,
        workflow_id,
        node_type,
        external_ref: Some(ExternalRef {
            ref_type: ExternalRefType::Intent,
            ref_id: Uuid::new_v4(),
        }),
        label: label.to_string(),
        properties: None,
    }
}

/// Helper: create an edge request
fn make_edge_request(
    tenant_id: Uuid,
    workflow_id: Uuid,
    from_id: Uuid,
    to_id: Uuid,
    edge_type: EdgeType,
) -> CreateGraphEdgeRequest {
    CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: from_id,
        to_node_id: to_id,
        edge_type,
        properties: None,
    }
}

/// Helper: build a chain graph (A -> B -> C -> ... -> Z)
/// Returns (service_arc, node_ids, start_id, end_id, workflow_id)
fn build_chain(n: usize) -> (Arc<GraphService>, Vec<Uuid>, Uuid, Uuid, Uuid) {
    let rt = Runtime::new().unwrap();
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo);
    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    let mut node_ids = Vec::with_capacity(n);
    for i in 0..n {
        let label = format!("node_{}", i);
        let node = rt
            .block_on(service.add_node(make_node_request(
                tenant_id,
                workflow_id,
                NodeType::Intent,
                &label,
            )))
            .unwrap();
        node_ids.push(node.id);
    }

    // Wire edges: node[i] -> node[i+1]
    for i in 0..n - 1 {
        rt.block_on(service.add_edge(make_edge_request(
            tenant_id,
            workflow_id,
            node_ids[i],
            node_ids[i + 1],
            EdgeType::DependsOn,
        )))
        .unwrap();
    }

    (
        Arc::new(service),
        node_ids.clone(),
        node_ids[0],
        node_ids[n - 1],
        workflow_id,
    )
}

/// Helper: build a diamond graph (A -> B, A -> C, B -> D, C -> D)
fn build_diamond() -> (Arc<GraphService>, Uuid, Uuid, Uuid, Uuid) {
    let rt = Runtime::new().unwrap();
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo);
    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    let a = rt
        .block_on(service.add_node(make_node_request(
            tenant_id,
            workflow_id,
            NodeType::Intent,
            "A",
        )))
        .unwrap();
    let b = rt
        .block_on(service.add_node(make_node_request(
            tenant_id,
            workflow_id,
            NodeType::Intent,
            "B",
        )))
        .unwrap();
    let c = rt
        .block_on(service.add_node(make_node_request(
            tenant_id,
            workflow_id,
            NodeType::Intent,
            "C",
        )))
        .unwrap();
    let d = rt
        .block_on(service.add_node(make_node_request(
            tenant_id,
            workflow_id,
            NodeType::Intent,
            "D",
        )))
        .unwrap();

    // A -> B, A -> C, B -> D, C -> D
    rt.block_on(service.add_edge(make_edge_request(
        tenant_id,
        workflow_id,
        a.id,
        b.id,
        EdgeType::DependsOn,
    )))
    .unwrap();
    rt.block_on(service.add_edge(make_edge_request(
        tenant_id,
        workflow_id,
        a.id,
        c.id,
        EdgeType::DependsOn,
    )))
    .unwrap();
    rt.block_on(service.add_edge(make_edge_request(
        tenant_id,
        workflow_id,
        b.id,
        d.id,
        EdgeType::DependsOn,
    )))
    .unwrap();
    rt.block_on(service.add_edge(make_edge_request(
        tenant_id,
        workflow_id,
        c.id,
        d.id,
        EdgeType::DependsOn,
    )))
    .unwrap();

    (Arc::new(service), a.id, b.id, c.id, d.id)
}

/// Helper: build a graph with a cycle (A -> B -> C -> A)
fn build_with_cycle() -> (Arc<GraphService>, Uuid, Uuid, Uuid, Uuid) {
    let rt = Runtime::new().unwrap();
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo);
    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    let a = rt
        .block_on(service.add_node(make_node_request(
            tenant_id,
            workflow_id,
            NodeType::Intent,
            "A",
        )))
        .unwrap();
    let b = rt
        .block_on(service.add_node(make_node_request(
            tenant_id,
            workflow_id,
            NodeType::Intent,
            "B",
        )))
        .unwrap();
    let c = rt
        .block_on(service.add_node(make_node_request(
            tenant_id,
            workflow_id,
            NodeType::Intent,
            "C",
        )))
        .unwrap();

    rt.block_on(service.add_edge(make_edge_request(
        tenant_id,
        workflow_id,
        a.id,
        b.id,
        EdgeType::DependsOn,
    )))
    .unwrap();
    rt.block_on(service.add_edge(make_edge_request(
        tenant_id,
        workflow_id,
        b.id,
        c.id,
        EdgeType::DependsOn,
    )))
    .unwrap();
    rt.block_on(service.add_edge(make_edge_request(
        tenant_id,
        workflow_id,
        c.id,
        a.id,
        EdgeType::DependsOn,
    )))
    .unwrap();

    (Arc::new(service), a.id, b.id, c.id, workflow_id)
}

// ============================================================================
// Benchmarks: find_reachable (BFS traversal)
// ============================================================================

fn bench_reachable_chain_unlimited(c: &mut Criterion) {
    let (service, _node_ids, start, _target, _workflow_id) = build_chain(20);
    let rt = Runtime::new().unwrap();

    c.bench_function("reachable_chain_unlimited_20", |b| {
        b.iter(|| {
            let result = rt.block_on(
                service.find_reachable(black_box(start), black_box(TraversalOptions::default())),
            );
            black_box(result)
        });
    });
}

fn bench_reachable_chain_depth_limited(c: &mut Criterion) {
    let (service, _node_ids, start, _target, _workflow_id) = build_chain(50);
    let rt = Runtime::new().unwrap();

    c.bench_function("reachable_chain_depth_limited_50", |b| {
        b.iter(|| {
            let result = rt.block_on(service.find_reachable(
                black_box(start),
                black_box(TraversalOptions {
                    max_depth: Some(5),
                    ..Default::default()
                }),
            ));
            black_box(result)
        });
    });
}

fn bench_reachable_diamond(c: &mut Criterion) {
    let (service, a_id, _, _, _d_id) = build_diamond();
    let rt = Runtime::new().unwrap();

    c.bench_function("reachable_diamond_a_to_d", |b| {
        b.iter(|| {
            let result = rt.block_on(
                service.find_reachable(black_box(a_id), black_box(TraversalOptions::default())),
            );
            black_box(result)
        });
    });
}

// ============================================================================
// Benchmarks: find_path (shortest path)
// ============================================================================

fn bench_path_chain(c: &mut Criterion) {
    let (service, _node_ids, start, end, _workflow_id) = build_chain(20);
    let rt = Runtime::new().unwrap();

    c.bench_function("path_chain_20", |b| {
        b.iter(|| {
            let result = rt.block_on(service.find_path(
                black_box(start),
                black_box(end),
                black_box(TraversalOptions::default()),
            ));
            black_box(result)
        });
    });
}

fn bench_path_diamond(c: &mut Criterion) {
    let (service, a_id, _, _, d_id) = build_diamond();
    let rt = Runtime::new().unwrap();

    c.bench_function("path_diamond_a_to_d", |b| {
        b.iter(|| {
            let result = rt.block_on(service.find_path(
                black_box(a_id),
                black_box(d_id),
                black_box(TraversalOptions::default()),
            ));
            black_box(result)
        });
    });
}

fn bench_path_no_route(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (service, _node_ids, start, _, _workflow_id) = build_chain(10);
    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create a second disconnected component
    let x = rt
        .block_on(service.add_node(make_node_request(
            tenant_id,
            workflow_id,
            NodeType::Intent,
            "X",
        )))
        .unwrap();
    let y = rt
        .block_on(service.add_node(make_node_request(
            tenant_id,
            workflow_id,
            NodeType::Intent,
            "Y",
        )))
        .unwrap();
    rt.block_on(service.add_edge(make_edge_request(
        tenant_id,
        workflow_id,
        x.id,
        y.id,
        EdgeType::DependsOn,
    )))
    .unwrap();

    c.bench_function("path_no_route_disconnected", |b| {
        b.iter(|| {
            let result = rt.block_on(service.find_path(
                black_box(start),
                black_box(y.id), // Different component - no path
                black_box(TraversalOptions::default()),
            ));
            black_box(result)
        });
    });
}

// ============================================================================
// Benchmarks: detect_cycles
// ============================================================================

fn bench_cycle_detection_no_cycle(c: &mut Criterion) {
    let (service, _, _, _, workflow_id) = build_chain(20);
    let rt = Runtime::new().unwrap();

    c.bench_function("cycle_detection_chain_no_cycle", |b| {
        b.iter(|| {
            let result = rt.block_on(service.detect_cycles(black_box(workflow_id)));
            black_box(result)
        });
    });
}

fn bench_cycle_detection_with_cycle(c: &mut Criterion) {
    let (service, _, _, _, workflow_id) = build_with_cycle();
    let rt = Runtime::new().unwrap();

    c.bench_function("cycle_detection_with_cycle", |b| {
        b.iter(|| {
            let result = rt.block_on(service.detect_cycles(black_box(workflow_id)));
            black_box(result)
        });
    });
}

// ============================================================================
// Benchmark group
// ============================================================================

criterion_group!(
    benches,
    // Reachable
    bench_reachable_chain_unlimited,
    bench_reachable_chain_depth_limited,
    bench_reachable_diamond,
    // Path
    bench_path_chain,
    bench_path_diamond,
    bench_path_no_route,
    // Cycle detection
    bench_cycle_detection_no_cycle,
    bench_cycle_detection_with_cycle,
);
criterion_main!(benches);
