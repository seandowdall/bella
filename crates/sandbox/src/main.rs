use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use clap::{Parser, ValueEnum};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Debug, Parser)]
#[command(name = "bella-sandbox")]
#[command(about = "Local deterministic mock provider APIs for Bella ingestion.")]
struct Args {
    #[arg(
        long,
        env = "BELLA_SANDBOX_BIND_ADDR",
        default_value = "127.0.0.1:4010"
    )]
    bind_addr: SocketAddr,

    #[arg(long, env = "BELLA_SANDBOX_SCENARIO", default_value = "happy-path")]
    scenario: Scenario,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum Scenario {
    HappyPath,
    Pagination,
    RateLimitOnce,
}

#[derive(Clone)]
struct AppState {
    scenario: Scenario,
    rate_limited: Arc<Mutex<HashSet<&'static str>>>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    scenario: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .compact()
        .init();

    let args = Args::parse();
    let state = AppState {
        scenario: args.scenario,
        rate_limited: Arc::new(Mutex::new(HashSet::new())),
    };
    let app = Router::new()
        .route("/health", get(health))
        .route(
            "/openai/v1/organization/usage/completions",
            get(openai_usage),
        )
        .route("/openai/v1/organization/costs", get(openai_costs))
        .with_state(state);

    let listener = TcpListener::bind(args.bind_addr).await?;
    tracing::info!(
        bind_addr = %args.bind_addr,
        scenario = ?args.scenario,
        openai_base_url = format!("http://{}/openai", args.bind_addr),
        "bella sandbox listening"
    );
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        scenario: format!("{:?}", state.scenario),
    })
}

async fn openai_usage(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    if let Some(response) = maybe_rate_limit(&state, "openai_usage") {
        return response;
    }

    let page = query.get("page").map(String::as_str);
    let bounds = bucket_bounds(&query);
    let payload = match state.scenario {
        Scenario::Pagination => openai_usage_paginated(page, bounds),
        Scenario::HappyPath | Scenario::RateLimitOnce => openai_usage_page(false, bounds),
    };
    Json(payload).into_response()
}

async fn openai_costs(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    if let Some(response) = maybe_rate_limit(&state, "openai_costs") {
        return response;
    }

    let page = query.get("page").map(String::as_str);
    let bounds = bucket_bounds(&query);
    let payload = match state.scenario {
        Scenario::Pagination => openai_costs_paginated(page, bounds),
        Scenario::HappyPath | Scenario::RateLimitOnce => openai_costs_page(false, bounds),
    };
    Json(payload).into_response()
}

fn maybe_rate_limit(state: &AppState, endpoint: &'static str) -> Option<Response> {
    if !matches!(state.scenario, Scenario::RateLimitOnce) {
        return None;
    }
    let mut rate_limited = state
        .rate_limited
        .lock()
        .expect("rate-limit state poisoned");
    if rate_limited.insert(endpoint) {
        let mut response = Json(json!({
            "error": {
                "message": "sandbox rate limit; retry this request",
                "type": "rate_limit_error"
            }
        }))
        .into_response();
        *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
        Some(response)
    } else {
        None
    }
}

fn bucket_bounds(query: &HashMap<String, String>) -> (i64, i64) {
    let requested_end = query
        .get("end_time")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(1_766_448_000);
    let end = requested_end;
    let start = end - 86_400;
    (start, end)
}

fn openai_usage_paginated(page: Option<&str>, bounds: (i64, i64)) -> Value {
    match page {
        Some("usage-page-2") => openai_usage_page(false, bounds),
        _ => openai_usage_page(true, bounds),
    }
}

fn openai_costs_paginated(page: Option<&str>, bounds: (i64, i64)) -> Value {
    match page {
        Some("costs-page-2") => openai_costs_page(false, bounds),
        _ => openai_costs_page(true, bounds),
    }
}

fn openai_usage_page(has_more: bool, (start_time, end_time): (i64, i64)) -> Value {
    json!({
        "object": "page",
        "data": [
            {
                "object": "bucket",
                "start_time": start_time,
                "end_time": end_time,
                "results": [
                    {
                        "object": "organization.usage.completions.result",
                        "input_tokens": 120000,
                        "output_tokens": 42000,
                        "num_model_requests": 84,
                        "model": "gpt-4o-mini",
                        "project_id": "proj_sandbox",
                        "user_id": "user_sandbox",
                        "api_key_id": "key_sandbox"
                    }
                ]
            }
        ],
        "has_more": has_more,
        "next_page": if has_more { json!("usage-page-2") } else { Value::Null }
    })
}

fn openai_costs_page(has_more: bool, (start_time, end_time): (i64, i64)) -> Value {
    json!({
        "object": "page",
        "data": [
            {
                "object": "bucket",
                "start_time": start_time,
                "end_time": end_time,
                "results": [
                    {
                        "object": "organization.costs.result",
                        "line_item": "completions",
                        "model": "gpt-4o-mini",
                        "project_id": "proj_sandbox",
                        "amount": {
                            "value": "0.184200",
                            "currency": "usd"
                        }
                    }
                ]
            }
        ],
        "has_more": has_more,
        "next_page": if has_more { json!("costs-page-2") } else { Value::Null }
    })
}
