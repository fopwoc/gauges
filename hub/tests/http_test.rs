mod support;

use std::{collections::BTreeMap, sync::Arc};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use gauges_hub::{HubDatabase, router};
use gauges_shared::{
    DeviceIndexResponse, DeviceSummarySyncRequest, DeviceSummarySyncResponse, ErrorResponse,
    HistorySyncRequest, HistorySyncResponse, IngestRequest,
};
use http_body_util::BodyExt;
use support::{test_config, test_identity, test_sample};
use tower::ServiceExt;

#[tokio::test]
async fn lazy_index_summary_and_history_contracts() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("hub.redb");
    let database = HubDatabase::open(&database_path).unwrap();
    let app = router(
        test_config(database_path, directory.path().into()),
        database,
        Arc::new(|| 10_000),
    );

    let index: DeviceIndexResponse = response_json(
        app.clone()
            .oneshot(
                Request::get("/api/v1/devices/index")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(index.total, 1);
    assert_eq!(index.online, 0);
    assert_eq!(index.offline, 1);
    assert_eq!(index.last_metric_time_ms, None);
    assert_eq!(index.device_ids, ["server-a"]);
    assert!(!index.revision.is_empty());

    let empty: DeviceSummarySyncResponse = post_json(
        app.clone(),
        "/api/v1/devices/sync",
        &DeviceSummarySyncRequest {
            device_ids: vec!["server-a".into()],
        },
        &[],
    )
    .await;
    assert_eq!(empty.devices["server-a"], None);

    let request = IngestRequest {
        identity: test_identity(),
        retention_hours: 168,
        collection_interval_seconds: 60,
        samples: vec![
            test_sample("sample-1", 1_000),
            test_sample("sample-2", 2_000),
        ],
    };
    let unauthorized = app
        .clone()
        .oneshot(json_request("/api/v1/ingest", &request, &[]))
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let ingested = app
        .clone()
        .oneshot(json_request(
            "/api/v1/ingest",
            &request,
            &[
                ("X-Gauges-Device-Id", "server-a"),
                ("Authorization", "Bearer secret"),
            ],
        ))
        .await
        .unwrap();
    assert_eq!(ingested.status(), StatusCode::OK);

    let index: DeviceIndexResponse = response_json(
        app.clone()
            .oneshot(
                Request::get("/api/v1/devices/index")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(index.online, 1);
    assert_eq!(index.offline, 0);
    assert_eq!(index.last_metric_time_ms, Some(2_000));

    let summary: DeviceSummarySyncResponse = post_json(
        app.clone(),
        "/api/v1/devices/sync",
        &DeviceSummarySyncRequest {
            device_ids: vec!["server-a".into()],
        },
        &[],
    )
    .await;
    let summary = summary.devices["server-a"].as_ref().unwrap();
    assert_eq!(summary.display_name, "Server A");
    assert_eq!(summary.retention_hours, 168);
    assert_eq!(summary.collection_interval_seconds, 60);
    assert_eq!(
        summary.latest_sample.as_ref().unwrap().sample_id,
        "sample-2"
    );
    assert!(summary.online);

    let history: HistorySyncResponse = post_json(
        app,
        "/api/v1/history/sync",
        &HistorySyncRequest {
            machines: BTreeMap::from([("server-a".into(), None)]),
        },
        &[],
    )
    .await;
    assert_eq!(
        history.machines["server-a"]
            .samples
            .iter()
            .map(|sample| sample.sample_id.as_str())
            .collect::<Vec<_>>(),
        ["sample-1", "sample-2"]
    );
    assert_eq!(
        history.machines["server-a"].next_metric_time_ms,
        Some(2_000)
    );
}

#[tokio::test]
async fn malformed_cursor_and_payload_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("hub.redb");
    let app = router(
        test_config(database_path.clone(), directory.path().into()),
        HubDatabase::open(&database_path).unwrap(),
        Arc::new(|| 10_000),
    );
    let cursor = app
        .clone()
        .oneshot(
            Request::post("/api/v1/history/sync")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"machines":{"server-a":-1}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cursor.status(), StatusCode::BAD_REQUEST);
    let malformed = app
        .oneshot(
            Request::post("/api/v1/ingest")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        malformed.headers()[header::CONTENT_TYPE],
        "application/json"
    );
    let error: ErrorResponse = response_json_with_status(malformed, StatusCode::BAD_REQUEST).await;
    assert_eq!(error.error, "invalid_request");
}

async fn post_json<RequestBody, ResponseBody>(
    app: axum::Router,
    path: &str,
    body: &RequestBody,
    headers: &[(&str, &str)],
) -> ResponseBody
where
    RequestBody: serde::Serialize,
    ResponseBody: serde::de::DeserializeOwned,
{
    response_json(
        app.oneshot(json_request(path, body, headers))
            .await
            .unwrap(),
    )
    .await
}

fn json_request<T: serde::Serialize>(
    path: &str,
    body: &T,
    headers: &[(&str, &str)],
) -> Request<Body> {
    let mut builder = Request::post(path).header(header::CONTENT_TYPE, "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    builder
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

async fn response_json<T: serde::de::DeserializeOwned>(response: axum::response::Response) -> T {
    response_json_with_status(response, StatusCode::OK).await
}

async fn response_json_with_status<T: serde::de::DeserializeOwned>(
    response: axum::response::Response,
    status: StatusCode,
) -> T {
    assert_eq!(response.status(), status);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}
