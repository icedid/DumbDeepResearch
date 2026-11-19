use axum::{extract::Query, response::Html, routing::get, Json, Router};
use ddr_engine::{CoordinatorConfig, ResearchCoordinator};
use ddr_knowledge::dossier::generate_audit_dossier;
use ddr_protocol::{ClientAction, EngineBroadcast, ExecutionMode, NodeStatus, Polarity};
use serde::Deserialize;
use std::time::Duration;
use tokio::sync::broadcast;

#[derive(Deserialize)]
struct SearchParams {
    q: String,
}

#[derive(Deserialize)]
struct AcParams {
    q: String,
}

#[tokio::test]
async fn end_to_end_crawl_equalizer_branching_and_audit() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock web server");
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{addr}");
    let base_for_search = base_url.clone();
    let base_for_page = base_url.clone();

    let app = Router::new()
        .route(
            "/autocompleter",
            get(|Query(params): Query<AcParams>| async move {
                let q = params.q;
                Json(serde_json::json!([
                    format!("{q} manufacturing cost"),
                    format!("{q} thermal safety")
                ]))
            }),
        )
        .route(
            "/search",
            get(move |Query(params): Query<SearchParams>| {
                let base = base_for_search.clone();
                async move {
                    let _ = params.q;
                    Json(serde_json::json!({
                        "results": [
                            {
                                "url": format!("{base}/article-1"),
                                "title": "Solid State Battery Breakthroughs & Bottlenecks",
                                "content": "Sulfide electrolytes achieve 450 Wh/kg energy density while dendrite growth remains a challenge."
                            },
                            {
                                "url": format!("{base}/article-2"),
                                "title": "Manufacturing Economics of Ceramic Separators",
                                "content": "Dry coating reduces solvent cost, however sintering pressure raises capital expense."
                            }
                        ]
                    }))
                }
            }),
        )
        .route(
            "/article-1",
            get(move || {
                let base = base_for_page.clone();
                async move {
                    Html(format!(
                        r#"<html>
                        <head><title>Solid State Battery Review</title><script>var ad=1;</script></head>
                        <body>
                          <nav><a href="/login">Sign in</a></nav>
                          <article>
                            <h1>Solid State Battery Engineering</h1>
                            <p>Solid state batteries achieve 450 Wh/kg gravimetric energy density and faster charging compared to conventional lithium-ion cells in recent laboratory tests. Sulfide solid electrolytes deliver superior thermal stability and eliminate flammable organic liquid solvents across 900 test cycles.</p>
                            <p>However, lithium dendrite propagation and interfacial mechanical cracking remain severe reliability bottlenecks during rapid cycling. High stack pressure of 50 MPa and moisture sensitivity create major manufacturing challenges and cost barriers for automotive pack integration. Researchers also observe sulfur gas evolution hazards when sulfide separators contact humid air.</p>
                            <p>Measured ionic conductivity reached 12 mS/cm at 25 °C across 1,200 cycles at $135 per kWh in pilot validation lines.<a href="{base}/study-citation.html">[1]</a></p>
                          </article>
                          <footer>Privacy Policy Copyright 2026</footer>
                        </body>
                        </html>"#
                    ))
                }
            }),
        )
        .route(
            "/article-2",
            get(|| async {
                Html(
                    r#"<html>
                    <head><title>Solid State Cell Manufacturing</title></head>
                    <body>
                      <article>
                        <p>Pilot production of solid state batteries improves volumetric efficiency by 35% and enables safer high-voltage cathode pairing for electric vehicles. Dry electrode processing achieves significant energy savings during cell assembly.</p>
                        <p>Despite these advantages, brittle oxide ceramic separators suffer from micro-fractures and supply chain shortages in precursor lithium sulfide materials. Yield losses above 18% and slow sintering throughput increase pack costs relative to established LFP factories.</p>
                      </article>
                    </body>
                    </html>"#
                        .to_string(),
                )
            }),
        )
        .route(
            "/study-citation.html",
            get(|| async {
                Html(
                    r#"<html>
                    <head><title>Primary Citation Benchmark Report</title></head>
                    <body>
                      <article>
                        <p>Independent testing on solid state batteries confirms 82% capacity retention after 1,000 cycles at 25 °C under 2C discharge rates. However, impedance growth across the cathode interface doubles after thermal shock testing at 60 °C, demonstrating persistent interfacial instability risks.</p>
                      </article>
                    </body>
                    </html>"#
                        .to_string(),
                )
            }),
        );

    let mock_task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let temp_dir = std::env::temp_dir().join(format!("ddr-e2e-{}", uuid::Uuid::new_v4()));
    let (tx, mut rx) = broadcast::channel(64);
    let cfg = CoordinatorConfig {
        search_endpoint: base_url,
        data_dir: Some(temp_dir.clone()),
        use_ollama: false,
        max_concurrent_fetches: 4,
        min_quality_words: 35,
    };
    let coordinator = ResearchCoordinator::with_config(tx, cfg);

    coordinator
        .handle_action(ClientAction::Start {
            topic: "solid state batteries".into(),
            max_depth: 2,
            max_sources: 3,
            mode: ExecutionMode::InteractiveSteered,
        })
        .await;

    let mut root_id = None;
    let mut completed = false;
    let mut proposed_children = Vec::new();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(250), rx.recv()).await {
            Ok(Ok(EngineBroadcast::NodeAdded { node })) => {
                if node.depth == 0 {
                    root_id = Some(node.id);
                } else {
                    proposed_children.push(node);
                }
            }
            Ok(Ok(EngineBroadcast::NodeUpdated { status, node, .. })) => {
                if Some(node.id) == root_id && status == NodeStatus::Complete {
                    completed = true;
                }
            }
            _ => {
                if completed && !proposed_children.is_empty() {
                    break;
                }
            }
        }
    }

    assert!(completed, "root node must reach Complete");
    let root_uuid = root_id.expect("root node ID recorded");
    assert!(
        !proposed_children.is_empty(),
        "must propose orthogonal child branches"
    );

    let graph = coordinator.graph.read().await;
    let root = graph.get_node(&root_uuid).expect("root in graph");
    assert!(!root.sources.is_empty(), "root must record verified sources");
    assert!(!root.findings.is_empty(), "root must extract balanced claims");
    assert!(
        root.findings.iter().any(|c| c.polarity == Polarity::Bull),
        "must include Bull claim"
    );
    assert!(
        root.findings.iter().any(|c| c.polarity == Polarity::Bear),
        "must include Bear claim"
    );
    assert!(
        root.sources.iter().all(|s| s.dom_sha256.len() == 64),
        "every source must have 64-char SHA-256"
    );

    let dossier = generate_audit_dossier(&graph);
    assert!(dossier.citation_audit.passed);
    assert!(dossier.citation_audit.total_citations >= 1);

    // Verify disk artifacts were written
    let raw_file = temp_dir.join("raw").join(format!("{root_uuid}.txt"));
    let json_file = temp_dir.join("nodes").join(format!("{root_uuid}.json"));
    assert!(raw_file.exists(), "raw dump must be persisted to disk");
    assert!(json_file.exists(), "node JSON must be persisted to disk");

    // Verify Explore on a Proposed child and Probe with custom query
    drop(graph);
    let first_child_id = proposed_children[0].id;
    coordinator
        .handle_action(ClientAction::Explore {
            node_id: first_child_id,
        })
        .await;

    let mut child_completed = false;
    let deadline2 = tokio::time::Instant::now() + Duration::from_secs(6);
    while tokio::time::Instant::now() < deadline2 {
        if let Ok(Ok(EngineBroadcast::NodeUpdated {
            node_id,
            status: NodeStatus::Complete,
            ..
        })) = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await
        {
            if node_id == first_child_id {
                child_completed = true;
                break;
            }
        }
    }
    assert!(child_completed, "explored child node must complete");

    // Verify Probe overrides query and re-executes
    coordinator
        .handle_action(ClientAction::Probe {
            node_id: first_child_id,
            custom_query: "solid state batteries dendrite risk".into(),
            prompt_override: Some("focus on failure modes".into()),
        })
        .await;

    let mut probed_completed = false;
    let deadline3 = tokio::time::Instant::now() + Duration::from_secs(6);
    while tokio::time::Instant::now() < deadline3 {
        if let Ok(Ok(EngineBroadcast::NodeUpdated {
            node_id,
            status: NodeStatus::Complete,
            node,
        })) = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await
        {
            if node_id == first_child_id && node.query == "solid state batteries dendrite risk" {
                probed_completed = true;
                break;
            }
        }
    }
    assert!(probed_completed, "probed child node must re-complete with custom query");

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    mock_task.abort();
}

#[tokio::test]
async fn autonomous_mode_auto_explores_and_prune_cancels_subtree() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock server");
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{addr}");
    let base_for_search = base_url.clone();

    let app = Router::new()
        .route(
            "/search",
            get(move || {
                let base = base_for_search.clone();
                async move {
                    Json(serde_json::json!({
                        "results": [
                            {
                                "url": format!("{base}/page-a"),
                                "title": "Solid State Electrolytes",
                                "content": "Sulfide solid state electrolytes and dendrite growth bottlenecks."
                            },
                            {
                                "url": format!("{base}/page-b"),
                                "title": "Solid State Manufacturing Cost",
                                "content": "Sintering pressure and manufacturing cost comparisons."
                            }
                        ]
                    }))
                }
            }),
        )
        .route(
            "/page-a",
            get(|| async {
                Html(
                    r#"<html><head><title>Electrolyte Study</title></head><body><article>
                    <p>Sulfide electrolytes achieve 450 Wh/kg energy density and improve safety across 900 cycles in solid state battery prototypes.</p>
                    <p>However, lithium dendrite penetration and interfacial cracking remain critical bottlenecks and reliability risks.</p>
                    </article></body></html>"#
                        .to_string(),
                )
            }),
        )
        .route(
            "/page-b",
            get(|| async {
                Html(
                    r#"<html><head><title>Manufacturing Economics</title></head><body><article>
                    <p>Dry electrode coating improves manufacturing efficiency by 30% for solid state battery factories.</p>
                    <p>Despite these gains, high stack pressure tooling increases capital cost and limits production yield.</p>
                    </article></body></html>"#
                        .to_string(),
                )
            }),
        );

    let mock_task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let (tx, mut rx) = broadcast::channel(128);
    let cfg = CoordinatorConfig {
        search_endpoint: base_url,
        data_dir: None,
        use_ollama: false,
        max_concurrent_fetches: 4,
        min_quality_words: 20,
    };
    let coordinator = ResearchCoordinator::with_config(tx, cfg);

    coordinator
        .handle_action(ClientAction::Start {
            topic: "solid state battery".into(),
            max_depth: 1,
            max_sources: 2,
            mode: ExecutionMode::AutonomousFree,
        })
        .await;

    let mut root_id = None;
    let mut completed_depth1 = 0usize;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    while tokio::time::Instant::now() < deadline {
        if let Ok(Ok(ev)) = tokio::time::timeout(Duration::from_millis(150), rx.recv()).await {
            match ev {
                EngineBroadcast::NodeAdded { node } if node.depth == 0 => {
                    root_id = Some(node.id);
                }
                EngineBroadcast::NodeUpdated {
                    status: NodeStatus::Complete,
                    node,
                    ..
                } if node.depth == 1 => {
                    completed_depth1 += 1;
                }
                _ => {}
            }
            if completed_depth1 >= 1 {
                break;
            }
        }
    }

    assert!(
        completed_depth1 >= 1,
        "AutonomousFree mode should auto-explore high-significance depth-1 branches"
    );

    // Pruning root must prune root and all descendants
    let root_uuid = root_id.unwrap();
    coordinator
        .handle_action(ClientAction::Prune { node_id: root_uuid })
        .await;

    let graph = coordinator.graph.read().await;
    assert!(graph
        .all_nodes()
        .iter()
        .all(|n| n.status == NodeStatus::Pruned));

    mock_task.abort();
}
