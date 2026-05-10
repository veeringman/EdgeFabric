use std::{
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use axum::{
    Json,
    extract::{Path as AxumPath, State},
    http::{HeaderValue, StatusCode, header},
    response::IntoResponse,
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::process::Command;
use uuid::Uuid;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::FileOptions};

use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;
use ef_db::repo::build::BuildJobRepo;
use ef_scheduler::{RunnerFilter, Scheduler};

#[derive(Debug, Deserialize)]
pub struct CreateIosStarterProjectRequest {
    pub name: String,
    #[serde(default)]
    pub organization_id: Option<String>,
    #[serde(default)]
    pub bundle_id: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub interface_style: Option<String>,
    #[serde(default)]
    pub deployment_target: Option<String>,
    #[serde(default)]
    pub archive: Option<bool>,
    /// SSH user to use when connecting to the runner host (defaults to current OS user)
    #[serde(default)]
    pub ssh_user: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateIosStarterProjectResponse {
    pub job_id: Uuid,
    pub module_name: String,
    pub project_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_path: Option<String>,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct IosStarterDefaultsResponse {
    pub username: String,
    pub suggested_app_name: String,
    pub suggested_organization_id: String,
    pub xcode_developer_dir: Option<String>,
    pub xcode_version: Option<String>,
    pub default_team_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WorkspaceProjectFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceArchiveExportRequest {
    #[serde(default)]
    pub project_name: Option<String>,
    pub files: Vec<WorkspaceProjectFile>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceArchiveImportRequest {
    pub archive_base64: String,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceArchiveImportResponse {
    pub files: Vec<WorkspaceProjectFile>,
    pub skipped_binary_files: usize,
}

fn trim_non_empty(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn shell_quote(s: &str) -> String {
    let escaped = s.replace('\'', "'\"'\"'");
    format!("'{}'", escaped)
}

fn sanitize_module_name(raw: &str) -> Option<String> {
    let mut module = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            module.push(ch);
        }
    }
    if module.is_empty() {
        None
    } else {
        Some(module)
    }
}

fn is_valid_module_name(module: &str) -> bool {
    !module.is_empty() && module.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_valid_workspace_path(path: &str) -> bool {
    if path.trim().is_empty() {
        return false;
    }
    if path.starts_with('/') || path.starts_with("\\") {
        return false;
    }
    !path.split('/').any(|part| part == ".." || part.is_empty())
}

fn resolve_project_root() -> PathBuf {
    if let Ok(root) = std::env::var("EF_PROJECT_ROOT") {
        let root = root.trim();
        if !root.is_empty() {
            return PathBuf::from(root);
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn generated_ios_dir(project_root: &Path) -> PathBuf {
    project_root.join("generated-ios")
}

async fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().await.ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

fn extract_team_id(identities: &str) -> Option<String> {
    for line in identities.lines() {
        let Some(start) = line.rfind('(') else {
            continue;
        };
        let Some(end) = line.rfind(')') else {
            continue;
        };
        if end <= start + 1 {
            continue;
        }
        let candidate = line[start + 1..end].trim();
        if candidate.len() == 10
            && candidate
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            return Some(candidate.to_string());
        }
    }
    None
}

pub async fn ios_starter_defaults(
    _auth: AuthContext,
    State(_state): State<AppState>,
) -> Result<Json<IosStarterDefaultsResponse>, (StatusCode, Json<ApiError>)> {
    let username = std::env::var("USER")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "developer".to_string());

    let mut app_name = username.clone();
    if !app_name.is_empty() {
        let mut chars = app_name.chars();
        if let Some(first) = chars.next() {
            app_name = format!("{}{}", first.to_ascii_uppercase(), chars.as_str());
        }
    }

    let xcode_developer_dir = command_stdout("xcode-select", &["-p"]).await;
    let xcode_version = command_stdout("xcodebuild", &["-version"]).await;
    let identities = command_stdout("security", &["find-identity", "-v", "-p", "codesigning"]).await;
    let default_team_id = identities.as_deref().and_then(extract_team_id);

    Ok(Json(IosStarterDefaultsResponse {
        username: username.clone(),
        suggested_app_name: format!("{}Starter", app_name),
        suggested_organization_id: format!("com.{}.edgefabric", username.to_lowercase()),
        xcode_developer_dir,
        xcode_version,
        default_team_id,
    }))
}

pub async fn create_ios_starter_project(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(payload): Json<CreateIosStarterProjectRequest>,
) -> Result<(StatusCode, Json<CreateIosStarterProjectResponse>), (StatusCode, Json<ApiError>)> {
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }

    let module_name = sanitize_module_name(&name)
        .ok_or_else(|| ApiError::bad_request("name must contain at least one alphanumeric character"))?;

    let interface_style = trim_non_empty(payload.interface_style).unwrap_or_else(|| "swiftui".to_string());
    if interface_style != "swiftui" && interface_style != "uikit" {
        return Err(ApiError::bad_request("interface_style must be one of: swiftui, uikit"));
    }

    // Create a build job for iOS starter project generation
    // Capture ssh_user from request, fallback to current OS username
    let ssh_user = payload.ssh_user
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            std::env::var("USER")
                .or_else(|_| std::env::var("LOGNAME"))
                .unwrap_or_else(|_| "runner".to_string())
        });

    let config_json = json!({
        "name": name,
        "organization_id": payload.organization_id,
        "bundle_id": payload.bundle_id,
        "team_id": payload.team_id,
        "interface_style": interface_style,
        "deployment_target": payload.deployment_target,
        "archive": payload.archive.unwrap_or(true),
    });

    let build = BuildJobRepo::create_ios(
        &state.pool,
        auth.tenant_id,
        &auth.tenant_name,
        "ios_starter",
        None,
        None,
        &config_json,
        &json!({}),
        None,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to create iOS starter build job");
        ApiError::internal("Failed to create build job")
    })?;

    let project_dir = generated_ios_dir(&resolve_project_root()).join(&module_name);
    let archive_path = project_dir.join(format!("{module_name}.zip"));

    // Try to allocate a macOS runner with xcode capabilities
    // For iOS starter, require xcode capability at minimum.
    // swift and macos are nice-to-have, but not strictly required.
    let mut required_caps = vec!["xcode".to_string()];

    let scheduler = Scheduler::new(state.pool.clone());
    let filter = RunnerFilter {
        region: None,
        required_capabilities: required_caps,
        required_labels: vec![],
        lease_ttl_seconds: 3600,
    };

    match scheduler
        .allocate_runner(auth.tenant_id, &filter, Some(build.id))
        .await
    {
        Ok((runner, lease)) => {
            let pool = state.pool.clone();
            let build_id = build.id;
            let runner_id = runner.id;
            let lease_id = lease.id;
            let runner_host = runner.host.clone();
            let runner_port = runner.port as i32;
            let runner_ssh_user = ssh_user.clone();
            
            // Spawn background task to run iOS starter script on remote runner
            tokio::spawn(async move {
                run_ios_starter_background(
                    pool,
                    build_id,
                    runner_id,
                    runner_host,
                    runner_port,
                    runner_ssh_user,
                    lease_id,
                    config_json,
                )
                .await
            });

            Ok((
                StatusCode::ACCEPTED,
                Json(CreateIosStarterProjectResponse {
                    job_id: build.id,
                    module_name,
                    project_dir: project_dir.display().to_string(),
                    archive_path: Some(archive_path.display().to_string()),
                    status: "provisioning".to_string(),
                    message: "iOS starter project generation queued for execution on remote runner".to_string(),
                }),
            ))
        }
        Err(e) => {
            let _ = BuildJobRepo::set_terminal(
                &state.pool,
                build.id,
                "failed",
                &json!({ "allocation_error": e.to_string() }),
                Some(&format!("Runner allocation failed: {e}")),
                None,
            )
            .await;
            
            Err(ApiError::not_found("No matching macOS runner with xcode capability available"))
        }
    }
}

async fn run_ios_starter_background(
    pool: sqlx::PgPool,
    build_id: Uuid,
    runner_id: Uuid,
    runner_host: String,
    runner_port: i32,
    runner_ssh_user: String,
    lease_id: Uuid,
    config: serde_json::Value,
) {
    let scheduler = Scheduler::new(pool.clone());
    
    tracing::info!(
        build_id = %build_id,
        runner_id = %runner_id,
        "Starting iOS starter project generation on remote runner"
    );

    let _ = BuildJobRepo::set_running(&pool, build_id).await;

    // Extract config parameters
    let name = config.get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("StarterApp");
    let organization_id = config.get("organization_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let bundle_id = config.get("bundle_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let team_id = config.get("team_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let interface_style = config.get("interface_style")
        .and_then(|v| v.as_str())
        .unwrap_or("swiftui");
    let deployment_target = config.get("deployment_target")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let archive = config.get("archive")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // Build remote script to run the iOS starter generator
    let remote_log_path = format!("/tmp/edgefabric-starter-{}.log", build_id);
    let remote_script = format!(
        "set -o pipefail; cd /tmp && (echo 'Starting iOS starter...' && {} --name {} --interface {} {} {} {} {} {} {} 2>&1) | tee {}",
        "/opt/edgefabric/scripts/create-ios-starter.sh",
        shell_quote(name),
        shell_quote(interface_style),
        if !organization_id.is_empty() { format!("--organization-id {}", shell_quote(organization_id)) } else { String::new() },
        if !bundle_id.is_empty() { format!("--bundle-id {}", shell_quote(bundle_id)) } else { String::new() },
        if !team_id.is_empty() { format!("--team-id {}", shell_quote(team_id)) } else { String::new() },
        if !deployment_target.is_empty() { format!("--deployment-target {}", shell_quote(deployment_target)) } else { String::new() },
        if archive { "--archive" } else { "" },
        format!("--output-dir /tmp/ios-starters"),
        shell_quote(&remote_log_path),
    );

    let output = ssh_exec(&runner_host, runner_port, &runner_ssh_user, &remote_script).await;

    let release_result = scheduler.release_runner_lease(lease_id).await;
    if let Err(e) = release_result {
        tracing::warn!(lease_id = %lease_id, error = %e, "Failed to release runner lease");
    }

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            let exit_code = out.status.code().unwrap_or(-1);
            let status = if out.status.success() { "succeeded" } else { "failed" };
            let error_msg = if out.status.success() {
                None
            } else {
                Some(format!("iOS starter generation failed with exit code {exit_code}"))
            };

            let runtime = json!({
                "exit_code": exit_code,
                "stdout": stdout,
                "stderr": stderr,
                "runner_id": runner_id,
                "remote_log_path": remote_log_path,
            });

            let _ = BuildJobRepo::set_terminal(
                &pool,
                build_id,
                status,
                &runtime,
                error_msg.as_deref(),
                None,
            )
            .await;

            tracing::info!(
                build_id = %build_id,
                status = %status,
                exit_code = exit_code,
                "iOS starter project generation completed"
            );
        }
        Err(e) => {
            let error_msg = format!("SSH execution failed: {e}");
            tracing::error!(
                build_id = %build_id,
                error = %error_msg,
                "iOS starter project generation SSH failed"
            );

            let _ = BuildJobRepo::set_terminal(
                &pool,
                build_id,
                "failed",
                &json!({
                    "runner_id": runner_id,
                    "error": error_msg.clone(),
                }),
                Some(&error_msg),
                None,
            )
            .await;
        }
    }
}

async fn ssh_exec(host: &str, port: i32, user: &str, script: &str) -> std::io::Result<std::process::Output> {
    let target = format!("{}@{}", user, host);
    Command::new("ssh")
        .args([
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "ConnectTimeout=15",
            "-o",
            "BatchMode=yes",
            "-p",
            &port.to_string(),
            &target,
            "sh",
            "-lc",
            script,
        ])
        .output()
        .await
}

pub async fn download_ios_starter_archive(
    _auth: AuthContext,
    State(_state): State<AppState>,
    AxumPath(module_name): AxumPath<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    if !is_valid_module_name(&module_name) {
        return Err(ApiError::bad_request("invalid starter project name"));
    }

    let archive_path = generated_ios_dir(&resolve_project_root()).join(format!("{module_name}.zip"));
    if !archive_path.exists() {
        return Err(ApiError::not_found(format!(
            "Starter archive not found for {}",
            module_name
        )));
    }

    let bytes = tokio::fs::read(&archive_path)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to read archive: {e}")))?;

    let content_disposition = HeaderValue::from_str(&format!(
        "attachment; filename=\"{module_name}.zip\""
    ))
    .map_err(|e| ApiError::internal(format!("Invalid content-disposition header: {e}")))?;

    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static("application/zip")),
        (header::CONTENT_DISPOSITION, content_disposition),
    ];

    Ok((headers, bytes))
}

pub async fn export_workspace_archive(
    _auth: AuthContext,
    State(_state): State<AppState>,
    Json(payload): Json<WorkspaceArchiveExportRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    if payload.files.is_empty() {
        return Err(ApiError::bad_request("files cannot be empty"));
    }

    let project_name = trim_non_empty(payload.project_name).unwrap_or_else(|| "edgefabric-workspace".to_string());
    let module_name = sanitize_module_name(&project_name)
        .ok_or_else(|| ApiError::bad_request("project_name must contain at least one alphanumeric character"))?;

    let cursor = Cursor::new(Vec::<u8>::new());
    let mut zip = ZipWriter::new(cursor);
    let options = FileOptions::default().compression_method(CompressionMethod::Deflated);

    for file in payload.files {
        if !is_valid_workspace_path(&file.path) {
            return Err(ApiError::bad_request(format!("invalid workspace file path: {}", file.path)));
        }

        zip.start_file(file.path, options)
            .map_err(|e| ApiError::internal(format!("Failed to start zip entry: {e}")))?;
        zip.write_all(file.content.as_bytes())
            .map_err(|e| ApiError::internal(format!("Failed to write zip entry: {e}")))?;
    }

    let bytes = zip
        .finish()
        .map_err(|e| ApiError::internal(format!("Failed to finalize zip archive: {e}")))?
        .into_inner();

    let content_disposition = HeaderValue::from_str(&format!(
        "attachment; filename=\"{module_name}.zip\""
    ))
    .map_err(|e| ApiError::internal(format!("Invalid content-disposition header: {e}")))?;

    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static("application/zip")),
        (header::CONTENT_DISPOSITION, content_disposition),
    ];

    Ok((headers, bytes))
}

pub async fn import_workspace_archive(
    _auth: AuthContext,
    State(_state): State<AppState>,
    Json(payload): Json<WorkspaceArchiveImportRequest>,
) -> Result<Json<WorkspaceArchiveImportResponse>, (StatusCode, Json<ApiError>)> {
    let archive_base64 = payload.archive_base64.trim();
    if archive_base64.is_empty() {
        return Err(ApiError::bad_request("archive_base64 is required"));
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(archive_base64)
        .map_err(|e| ApiError::bad_request(format!("Invalid archive_base64 payload: {e}")))?;

    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| ApiError::bad_request(format!("Invalid zip archive: {e}")))?;

    let mut files = Vec::new();
    let mut skipped_binary_files = 0usize;

    for idx in 0..zip.len() {
        let mut entry = zip
            .by_index(idx)
            .map_err(|e| ApiError::internal(format!("Failed to read zip entry: {e}")))?;

        if entry.is_dir() {
            continue;
        }

        let path = entry.name().replace('\\', "/");
        if !is_valid_workspace_path(&path) {
            continue;
        }

        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| ApiError::internal(format!("Failed reading zip content: {e}")))?;

        match String::from_utf8(buf) {
            Ok(content) => files.push(WorkspaceProjectFile { path, content }),
            Err(_) => skipped_binary_files += 1,
        }
    }

    Ok(Json(WorkspaceArchiveImportResponse {
        files,
        skipped_binary_files,
    }))
}

// ─── Saved workspace projects (server-side persistence) ──────────────────────

#[derive(Debug, Deserialize)]
pub struct SaveWorkspaceProjectRequest {
    pub name: String,
    pub files: Vec<WorkspaceProjectFile>,
}

#[derive(Debug, Serialize)]
pub struct SaveWorkspaceProjectResponse {
    pub id: Uuid,
    pub name: String,
    pub file_count: i32,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceProjectSummary {
    pub id: Uuid,
    pub name: String,
    pub file_count: i32,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct ListWorkspaceProjectsResponse {
    pub projects: Vec<WorkspaceProjectSummary>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceProjectDetail {
    pub id: Uuid,
    pub name: String,
    pub files: Vec<WorkspaceProjectFile>,
    pub file_count: i32,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

fn validate_project_name(raw: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request("project name cannot be empty"));
    }
    if trimmed.len() > 128 {
        return Err(ApiError::bad_request("project name is too long (max 128 chars)"));
    }
    if !trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == ' ') {
        return Err(ApiError::bad_request(
            "project name may only contain alphanumerics, space, '_', '-' or '.'",
        ));
    }
    Ok(trimmed.to_string())
}

pub async fn save_workspace_project(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(payload): Json<SaveWorkspaceProjectRequest>,
) -> Result<Json<SaveWorkspaceProjectResponse>, (StatusCode, Json<ApiError>)> {
    use sqlx::Row;

    let name = validate_project_name(&payload.name)?;

    for file in &payload.files {
        if !is_valid_workspace_path(&file.path) {
            return Err(ApiError::bad_request(format!(
                "invalid workspace file path: {}",
                file.path
            )));
        }
    }

    let files_json = serde_json::to_value(&payload.files)
        .map_err(|e| ApiError::internal(format!("Failed to serialize files: {e}")))?;
    let file_count = payload.files.len() as i32;

    let row = sqlx::query(
        r#"
        INSERT INTO workspace_projects (tenant_id, name, files, file_count)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (tenant_id, name) DO UPDATE
            SET files = EXCLUDED.files,
                file_count = EXCLUDED.file_count,
                updated_at = NOW()
        RETURNING id, name, file_count, created_at, updated_at
        "#,
    )
    .bind(auth.tenant_id)
    .bind(&name)
    .bind(&files_json)
    .bind(file_count)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to save project: {e}")))?;

    Ok(Json(SaveWorkspaceProjectResponse {
        id: row.try_get("id").map_err(|e| ApiError::internal(e.to_string()))?,
        name: row.try_get("name").map_err(|e| ApiError::internal(e.to_string()))?,
        file_count: row.try_get("file_count").map_err(|e| ApiError::internal(e.to_string()))?,
        created_at: row.try_get("created_at").map_err(|e| ApiError::internal(e.to_string()))?,
        updated_at: row.try_get("updated_at").map_err(|e| ApiError::internal(e.to_string()))?,
    }))
}

pub async fn list_workspace_projects(
    auth: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<ListWorkspaceProjectsResponse>, (StatusCode, Json<ApiError>)> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"
        SELECT id, name, file_count, created_at, updated_at
        FROM workspace_projects
        WHERE tenant_id = $1
        ORDER BY updated_at DESC
        "#,
    )
    .bind(auth.tenant_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to list projects: {e}")))?;

    let mut projects = Vec::with_capacity(rows.len());
    for r in rows {
        projects.push(WorkspaceProjectSummary {
            id: r.try_get("id").map_err(|e| ApiError::internal(e.to_string()))?,
            name: r.try_get("name").map_err(|e| ApiError::internal(e.to_string()))?,
            file_count: r.try_get("file_count").map_err(|e| ApiError::internal(e.to_string()))?,
            created_at: r.try_get("created_at").map_err(|e| ApiError::internal(e.to_string()))?,
            updated_at: r.try_get("updated_at").map_err(|e| ApiError::internal(e.to_string()))?,
        });
    }

    Ok(Json(ListWorkspaceProjectsResponse { projects }))
}

pub async fn get_workspace_project(
    auth: AuthContext,
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Result<Json<WorkspaceProjectDetail>, (StatusCode, Json<ApiError>)> {
    use sqlx::Row;

    let name = validate_project_name(&name)?;

    let row = sqlx::query(
        r#"
        SELECT id, name, files, file_count, created_at, updated_at
        FROM workspace_projects
        WHERE tenant_id = $1 AND name = $2
        "#,
    )
    .bind(auth.tenant_id)
    .bind(&name)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to load project: {e}")))?;

    let row = row.ok_or_else(|| ApiError::not_found("project not found"))?;

    let files_json: serde_json::Value = row
        .try_get("files")
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let files: Vec<WorkspaceProjectFile> = serde_json::from_value(files_json)
        .map_err(|e| ApiError::internal(format!("Failed to decode stored files: {e}")))?;

    Ok(Json(WorkspaceProjectDetail {
        id: row.try_get("id").map_err(|e| ApiError::internal(e.to_string()))?,
        name: row.try_get("name").map_err(|e| ApiError::internal(e.to_string()))?,
        files,
        file_count: row.try_get("file_count").map_err(|e| ApiError::internal(e.to_string()))?,
        created_at: row.try_get("created_at").map_err(|e| ApiError::internal(e.to_string()))?,
        updated_at: row.try_get("updated_at").map_err(|e| ApiError::internal(e.to_string()))?,
    }))
}

pub async fn delete_workspace_project(
    auth: AuthContext,
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let name = validate_project_name(&name)?;

    let result = sqlx::query(
        r#"DELETE FROM workspace_projects WHERE tenant_id = $1 AND name = $2"#,
    )
    .bind(auth.tenant_id)
    .bind(&name)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to delete project: {e}")))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("project not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}
