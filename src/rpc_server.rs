use anyhow::Result;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Redirect, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::collections::HashMap;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

use crate::types::{RpcPackageDetails, RpcPackageInfo};
use crate::{
    app_state::AppState,
    database::DatabaseOps,
    types::{RpcResponse, SearchType},
};

#[derive(Clone)]
pub struct RpcState {
    db: DatabaseOps,
    client: reqwest::Client,
    github_token: Option<String>,
}

pub struct RpcServer {
    app: Router,
}

#[derive(Debug, Deserialize)]
struct RpcQuery {
    v: Option<String>,
    #[serde(rename = "type")]
    request_type: Option<String>,
    #[serde(rename = "by")]
    search_by: Option<String>,
    #[serde(default, rename = "arg")]
    args0: Vec<String>,
    #[serde(default, rename = "arg[]")]
    args1: Vec<String>,
    callback: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RpcForm {
    v: Option<String>,
    #[serde(rename = "type")]
    request_type: Option<String>,
    #[serde(rename = "by")]
    search_by: Option<String>,
    #[serde(default, rename = "arg")]
    args0: Vec<String>,
    #[serde(default, rename = "arg[]")]
    args1: Vec<String>,
}

impl RpcServer {
    pub fn new(app_state: AppState) -> Self {
        let state = RpcState {
            db: app_state.db,
            client: reqwest::Client::new(),
            github_token: app_state.github_token,
        };

        let app = Router::new()
            .route("/rpc", get(handle_rpc_get))
            .route("/rpc", post(handle_rpc_post))
            .route(
                "/cgit/aur.git/snapshot/{snapshot_name}",
                get(handle_snapshot),
            )
            .route("/{branch}/info/refs", get(handle_git_info_refs))
            .route(
                "/{branch}/git-upload-pack",
                post(handle_git_upload_pack_post),
            )
            .layer(CorsLayer::permissive())
            .with_state(state);

        Self { app }
    }

    pub async fn run(self, addrs: impl Iterator<Item = impl AsRef<str>>) -> Result<()> {
        futures::future::try_join_all(addrs.map(async |addr| -> Result<()> {
            info!("Listening on http://{}", addr.as_ref());
            let listener = tokio::net::TcpListener::bind(addr.as_ref()).await?;
            axum::serve(listener, self.app.clone()).await?;
            Ok(())
        }))
        .await?;
        Ok(())
    }
}

async fn handle_rpc_get(
    State(state): State<RpcState>,
    axum_extra::extract::Query(query): axum_extra::extract::Query<RpcQuery>,
) -> Result<Response<String>, StatusCode> {
    let all_args = query.args0.into_iter().chain(query.args1).collect();

    handle_rpc_request(
        query.v,
        query.request_type,
        query.search_by,
        all_args,
        query.callback,
        state,
    )
    .await
}

async fn handle_rpc_post(
    State(state): State<RpcState>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<RpcForm>,
) -> Result<Response<String>, StatusCode> {
    let all_args = form.args0.into_iter().chain(form.args1).collect();

    handle_rpc_request(
        form.v,
        form.request_type,
        form.search_by,
        all_args,
        None, // POST doesn't support JSONP
        state,
    )
    .await
}

async fn handle_rpc_request(
    version: Option<String>,
    request_type: Option<String>,
    search_by: Option<String>,
    args: Vec<String>,
    callback: Option<String>,
    state: RpcState,
) -> Result<Response<String>, StatusCode> {
    // Validate version
    let version_num = match version {
        None => {
            let error = error_response("Please specify an API version.".to_string(), None);
            return Ok(create_response(&error, callback));
        }
        Some(v) => match v.as_str() {
            "5" => 5,
            _ => {
                let parsed_version = v.parse::<u32>().ok();
                let error =
                    error_response("Invalid version specified.".to_string(), parsed_version);
                return Ok(create_response(&error, callback));
            }
        },
    };

    // Validate request type
    let req_type = match request_type {
        None => {
            let error = error_response(
                "No request type/data specified.".to_string(),
                Some(version_num),
            );
            return Ok(create_response(&error, callback));
        }
        Some(t) => t,
    };

    match req_type.as_str() {
        "search" => {
            handle_search(
                state,
                search_by,
                args.first().map(|s| s.as_str()).unwrap_or(""),
                callback,
            )
            .await
        }
        "info" => handle_info(state, args, callback).await,
        _ => {
            let error = error_response(
                "Incorrect request type specified.".to_string(),
                Some(version_num),
            );
            Ok(create_response(&error, callback))
        }
    }
}

async fn handle_search(
    state: RpcState,
    search_by: Option<String>,
    keyword: &str,
    callback: Option<String>,
) -> Result<Response<String>, StatusCode> {
    if keyword.is_empty() {
        let error = error_response("Query arg too small.".to_string(), Some(5));
        return Ok(create_response(&error, callback));
    }

    let search_type = search_by.as_deref().unwrap_or("name-desc");
    let search_enum = SearchType::from_str(search_type);
    if search_enum.is_none() {
        let error = error_response("Incorrect by field specified.".to_string(), Some(5));
        return Ok(create_response(&error, callback));
    }
    let search_enum = search_enum.unwrap();

    match state.db.search_packages(search_enum, keyword).await {
        Ok(rows) => {
            let results: Vec<RpcPackageInfo> = rows
                .into_iter()
                .map(|row| RpcPackageInfo {
                    id: 0,
                    name: row.pkg_name.clone(),
                    description: row.pkg_desc.clone().unwrap_or_default(),
                    package_base: row.branch.clone(),
                    package_base_id: 0,
                    version: row.version.clone(),
                    url: row.url.clone().unwrap_or_default(),
                    url_path: format!("/cgit/aur.git/snapshot/{}.tar.gz", row.branch),
                    maintainer: row.maintainer.unwrap_or_default(),
                    num_votes: row.num_votes.unwrap_or(0) as u32,
                    popularity: row.popularity.unwrap_or(0.0),
                    first_submitted: row.first_submitted.unwrap_or(0),
                    last_modified: row.last_modified.unwrap_or(0),
                    out_of_date: row.out_of_date,
                })
                .collect();

            let response = RpcResponse {
                error: None,
                result_count: results.len(),
                results,
                response_type: "search".to_string(),
                version: Some(5),
            };

            Ok(create_response(&response, callback))
        }
        Err(e) => {
            error!("Database error during search: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_info(
    state: RpcState,
    args: Vec<String>,
    callback: Option<String>,
) -> Result<Response<String>, StatusCode> {
    if args.is_empty() {
        let error = error_response("No request type/data specified.".to_string(), Some(5));
        return Ok(create_response(&error, callback));
    }

    match state.db.get_package_details(&args).await {
        Ok(package_details) => {
            let results: Vec<RpcPackageDetails> = package_details
                .into_iter()
                .map(|details| RpcPackageDetails {
                    id: 0,
                    name: details.info.pkg_name.clone(),
                    description: details.info.pkg_desc.clone(),
                    package_base: details.info.branch.clone(),
                    package_base_id: 0,
                    version: details.info.version.clone(),
                    url: details.info.url.clone(),
                    url_path: format!("/cgit/aur.git/snapshot/{}.tar.gz", details.info.branch),
                    maintainer: details.info.maintainer,
                    submitter: details.info.submitter,
                    num_votes: details.info.num_votes.unwrap_or(0) as u32,
                    popularity: details.info.popularity.unwrap_or(0.0),
                    first_submitted: details.info.first_submitted.unwrap_or(0),
                    last_modified: details.info.last_modified.unwrap_or(0),
                    out_of_date: details.info.out_of_date,
                    license: Vec::new(),
                    depends: details.depends,
                    makedepends: details.make_depends,
                    optdepends: details.opt_depends,
                    checkdepends: details.check_depends,
                    provides: details.provides,
                    conflicts: details.conflicts,
                    replaces: details.replaces,
                    groups: details.groups,
                    keywords: details.keywords,
                    co_maintainers: details.co_maintainers,
                })
                .collect();

            let response = RpcResponse {
                error: None,
                result_count: results.len(),
                results,
                response_type: "multiinfo".to_string(),
                version: Some(5),
            };

            Ok(create_response(&response, callback))
        }
        Err(e) => {
            error!("Database error during info lookup: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_snapshot(
    State(state): State<RpcState>,
    Path(snapshot_name): Path<String>,
) -> Result<Redirect, StatusCode> {
    let branch_name = snapshot_name.strip_suffix(".tar.gz");

    if let Some(branch_name) = branch_name {
        match state.db.get_branch_commit_id(branch_name).await {
            Ok(Some(commit_id)) => {
                let github_url = format!(
                    "https://github.com/archlinux/aur/archive/{}.tar.gz",
                    commit_id
                );
                Ok(Redirect::temporary(&github_url))
            }
            Ok(None) => Err(StatusCode::NOT_FOUND),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn handle_git_info_refs(
    State(state): State<RpcState>,
    Path(branch): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response<String>, StatusCode> {
    // Remove .git extension if present
    let branch_name = branch.strip_suffix(".git").unwrap_or(&branch);

    let service = match params.get("service") {
        Some(s) => s,
        None => {
            return Ok(Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body("Please upgrade your git client.".to_string())
                .unwrap());
        }
    };

    if service != "git-upload-pack" {
        return Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body("Unsupported service".to_string())
            .unwrap());
    }

    // Check if package exists and get commit ID
    match state.db.get_branch_commit_id(branch_name).await {
        Ok(Some(commit_id)) => {
            let response_body = format!("001e# service=git-upload-pack\n000000e1{} HEAD\u{0000}multi_ack thin-pack side-band side-band-64k ofs-delta no-progress include-tag multi_ack_detailed no-done symref=HEAD:refs/heads/master object-format=sha1 agent=git/aur-mirror\n003f{} refs/heads/master\n0000",
                commit_id,
                commit_id
            );

            Ok(Response::builder()
                .header(
                    header::CONTENT_TYPE,
                    "application/x-git-upload-pack-advertisement",
                )
                .body(response_body)
                .unwrap())
        }
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn error_response(message: String, version: Option<u32>) -> RpcResponse<()> {
    RpcResponse::<()> {
        error: Some(message),
        result_count: 0,
        results: Vec::new(),
        response_type: "error".to_string(),
        version,
    }
}

fn create_response<T: serde::Serialize>(data: &T, callback: Option<String>) -> Response<String> {
    let json = serde_json::to_string(data).unwrap();

    if let Some(callback_fn) = callback {
        // JSONP response
        let jsonp = format!("{}({});", callback_fn, json);
        Response::builder()
            .header(header::CONTENT_TYPE, "application/javascript")
            .body(jsonp)
            .unwrap()
    } else {
        // Regular JSON response
        Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(json)
            .unwrap()
    }
}

async fn handle_git_upload_pack_post(
    State(state): State<RpcState>,
    Path(branch): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response<Body>, StatusCode> {
    let branch_name = branch.strip_suffix(".git").unwrap_or(&branch);

    // Check if package exists and get commit ID
    match state.db.get_branch_commit_id(branch_name).await {
        Ok(Some(_)) => {
            let mut req = state
                .client
                .post("https://github.com/archlinux/aur.git/git-upload-pack");
            for (key, value) in headers.iter() {
                match *key {
                    header::HOST => {
                        // Skip
                    }
                    header::AUTHORIZATION => {
                        // Skip
                    }
                    _ => {
                        req = req.header(key, value.clone());
                    }
                }
            }
            if let Some(token) = state.github_token.as_deref() {
                req = req.basic_auth(token, None::<&str>);
            }
            let upstream = req
                .body(reqwest::Body::wrap_stream(body.into_data_stream()))
                .send()
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut response_builder = Response::builder().status(upstream.status());
            *response_builder.headers_mut().unwrap() = upstream.headers().clone();
            response_builder
                .body(Body::from_stream(upstream.bytes_stream()))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        }
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
