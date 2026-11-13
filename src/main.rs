use axum::{
    extract::DefaultBodyLimit,
    extract::State,
    http::{header, HeaderMap},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    env, fs,
    process::{Command, Stdio},
    sync::Arc,
    time::Instant,
};
use tokio::sync::Mutex;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command as TokioCommand,
    time::timeout,
};

const LOCAL_URL: &str = "http://127.0.0.1:8080/v1";
const NVIDIA_URL: &str = "https://integrate.api.nvidia.com/v1";
const NVIDIA_MODEL: &str = "nvidia/nemotron-3.5-lightning-30b-a3b";
const NVIDIA_FAST_MODEL: &str = "nvidia/nemotron-3.5-bolt-e4b";
const NVIDIA_HARD_MODEL: &str = "deepseek-ai/deepseek-v4.1-flash";
// NVIDIA's catalog marks these hosted endpoints as "Free Endpoint". This is
// an allowlist, not a list of every model returned by /v1/models.
const VERIFIED_FREE_NVIDIA_MODELS: &[&str] = &[
    "nvidia/nemotron-3.5-bolt-e4b",
    "nvidia/nemotron-3.5-lightning-30b-a3b",
    "deepseek-ai/deepseek-v4.1-flash",
    "z-ai/glm-5.3-flash",
    "minimaxai/minimax-m3.1-flash",
    "meta/muse-glimmer-30b",
    "nvidia/nemotron-3.5-super-vl-120b-a12b",
    "mistralai/mistral-large-4-1t-a52b",
    "stepfun-ai/step-5-preview",
];
const DASHBOARD: &str = include_str!("ui.html");

#[derive(Parser, Debug)]
#[command(name = "thomas-ai", about = "Thomas AI for Apple Silicon")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 8788)]
    port: u16,
    #[arg(long, env = "AI_PROVIDER", default_value = "auto")]
    provider: String,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Serve,
    Status,
    Models,
    Chat {
        prompt: Vec<String>,
        #[arg(long, default_value = "auto")]
        provider: String,
        #[arg(long)]
        difficulty: Option<u8>,
    },
    MlxStart {
        #[arg(long)]
        model: String,
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    Cluster {
        #[arg(default_value = "status")]
        action: String,
    },
    Exec {
        #[arg(trailing_var_arg = true)]
        command: Vec<String>,
    },
}

#[derive(Clone)]
struct AppState {
    cfg: Arc<Config>,
    router_model: Option<RouterModel>,
    http: Client,
    stats: Arc<Mutex<Stats>>,
    local_inflight: Arc<std::sync::atomic::AtomicUsize>,
    nvidia_inflight: Arc<std::sync::atomic::AtomicUsize>,
    nvidia_health: Arc<Mutex<HealthCache>>,
    apple_inflight: Arc<std::sync::atomic::AtomicUsize>,
}

#[derive(Default)]
struct HealthCache {
    ok: bool,
    checked_at: Option<Instant>,
}

struct InflightGuard {
    counter: Arc<std::sync::atomic::AtomicUsize>,
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        self.counter
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

#[derive(Clone)]
struct Config {
    provider: String,
    local_url: String,
    nvidia_url: String,
    local_model: String,
    nvidia_model: String,
    nvidia_fast_model: String,
    nvidia_hard_model: String,
    nvidia_free_models: Vec<String>,
    apple_bin: String,
    apple_mode: String,
    local_max_difficulty: u8,
}

#[derive(Clone, Deserialize)]
struct RouterModel {
    classes: Vec<String>,
    means: Vec<f64>,
    scales: Vec<f64>,
    weights: Vec<Vec<f64>>,
    bias: Vec<f64>,
}

struct RouterPrediction {
    provider: String,
    confidence: f64,
}

#[derive(Default, Serialize)]
struct Stats {
    local_requests: u64,
    nvidia_requests: u64,
    local_latency_ms: f64,
    nvidia_latency_ms: f64,
    local_failures: u64,
    nvidia_failures: u64,
    nvidia_timeouts: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Message {
    role: String,
    content: Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ChatRequest {
    model: Option<String>,
    messages: Vec<Message>,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    extra_body: Option<Value>,
    #[serde(flatten)]
    rest: serde_json::Map<String, Value>,
}

fn config(provider: String) -> Config {
    let configured_free = env::var("NVIDIA_FREE_MODELS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|model| VERIFIED_FREE_NVIDIA_MODELS.contains(model))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|models| !models.is_empty())
        .unwrap_or_else(|| {
            VERIFIED_FREE_NVIDIA_MODELS
                .iter()
                .map(|model| (*model).into())
                .collect()
        });
    let default_model = env::var("NVIDIA_MODEL").unwrap_or_else(|_| NVIDIA_MODEL.into());
    let fast_model = env::var("NVIDIA_FAST_MODEL").unwrap_or_else(|_| NVIDIA_FAST_MODEL.into());
    let hard_model = env::var("NVIDIA_HARD_MODEL").unwrap_or_else(|_| NVIDIA_HARD_MODEL.into());
    let safe_default = if configured_free.contains(&default_model) {
        default_model
    } else {
        NVIDIA_MODEL.into()
    };
    let safe_fast = if configured_free.contains(&fast_model) {
        fast_model
    } else {
        NVIDIA_FAST_MODEL.into()
    };
    let safe_hard = if configured_free.contains(&hard_model) {
        hard_model
    } else {
        NVIDIA_HARD_MODEL.into()
    };
    Config {
        provider,
        local_url: env::var("MLX_BASE_URL")
            .unwrap_or_else(|_| LOCAL_URL.into())
            .trim_end_matches('/')
            .into(),
        nvidia_url: env::var("NVIDIA_BASE_URL")
            .unwrap_or_else(|_| NVIDIA_URL.into())
            .trim_end_matches('/')
            .into(),
        local_model: env::var("MLX_MODEL").unwrap_or_default(),
        nvidia_model: safe_default,
        nvidia_fast_model: safe_fast,
        nvidia_hard_model: safe_hard,
        nvidia_free_models: configured_free,
        apple_bin: env::var("THOMAS_APPLE_AI_BIN").unwrap_or_else(|_| "/Users/local.local/Projects/TradingSystem/thomas-ai-trading-core/apple-ai-bridge/.build/release/thomas-apple-ai".into()),
        apple_mode: env::var("THOMAS_APPLE_AI_MODE").unwrap_or_else(|_| "private_cloud".into()),
        local_max_difficulty: env::var("AI_LOCAL_MAX_DIFFICULTY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3),
    }
}

fn load_router_model() -> Option<RouterModel> {
    let path = env::var("THOMAS_ROUTER_MODEL").ok()?;
    let bytes = fs::read(path).ok()?;
    let model: RouterModel = serde_json::from_slice(&bytes).ok()?;
    let valid_classes = model
        .classes
        .iter()
        .all(|class| matches!(class.as_str(), "local" | "nvidia" | "apple"));
    let feature_count = model.means.len();
    let valid_shape = !model.classes.is_empty()
        && feature_count == model.scales.len()
        && model.weights.len() == model.classes.len()
        && model
            .weights
            .iter()
            .all(|weights| weights.len() == feature_count)
        && model.bias.len() == model.classes.len();
    let finite = model
        .means
        .iter()
        .chain(model.scales.iter())
        .chain(model.bias.iter())
        .chain(model.weights.iter().flatten())
        .all(|value| value.is_finite());
    (valid_classes && valid_shape && finite).then_some(model)
}

fn router_features(req: &ChatRequest) -> Vec<f64> {
    let text = req
        .messages
        .iter()
        .filter_map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let lower = text.to_lowercase();
    let words = lower.split_whitespace().count() as f64;
    let count_terms = |terms: &[&str]| -> f64 {
        terms
            .iter()
            .map(|term| lower.matches(term).count() as f64)
            .sum()
    };
    vec![
        (text.len().min(12_000) as f64) / 12_000.0,
        words.min(2_400.0) / 2_400.0,
        (text.matches('?').count().min(10) as f64) / 10.0,
        (text.matches('\n').count().min(100) as f64) / 100.0,
        if text.contains("```") || lower.contains("traceback") {
            1.0
        } else {
            0.0
        },
        count_terms(&[
            "run", "execute", "edit", "write", "shell", "command", "terminal", "git",
        ]) / 8.0,
        count_terms(&[
            "architecture",
            "debug",
            "refactor",
            "prove",
            "derive",
            "compare",
            "optimize",
        ]) / 6.0,
        count_terms(&["private", "offline", "local", "mac", "file", "repository"]) / 6.0,
    ]
}

fn learned_provider(model: &RouterModel, req: &ChatRequest) -> Option<RouterPrediction> {
    if model.classes.is_empty()
        || model.means.len() != model.scales.len()
        || model.weights.len() != model.classes.len()
        || model.bias.len() != model.classes.len()
    {
        return None;
    }
    let values = router_features(req);
    let normalized = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let mean = model.means.get(index).copied().unwrap_or(0.0);
            let scale = model.scales.get(index).copied().unwrap_or(1.0);
            (value - mean) / if scale == 0.0 { 1.0 } else { scale }
        })
        .collect::<Vec<_>>();
    let scores = model
        .weights
        .iter()
        .zip(model.bias.iter())
        .enumerate()
        .map(|(index, (weights, bias))| {
            (
                index,
                *bias
                    + weights
                        .iter()
                        .zip(normalized.iter())
                        .map(|(w, v)| w * v)
                        .sum::<f64>(),
            )
        })
        .map(|(_, score)| score)
        .collect::<Vec<_>>();
    let peak = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let probabilities = scores
        .iter()
        .map(|score| (score - peak).exp())
        .collect::<Vec<_>>();
    let total = probabilities.iter().sum::<f64>();
    probabilities
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .and_then(|(index, probability)| {
            model
                .classes
                .get(index)
                .cloned()
                .map(|provider| RouterPrediction {
                    provider,
                    confidence: probability / total,
                })
        })
}

fn routed_nvidia_model(state: &AppState, score: u8) -> String {
    let candidate = if score <= 2 {
        state.cfg.nvidia_fast_model.clone()
    } else if score >= 7 {
        state.cfg.nvidia_hard_model.clone()
    } else {
        state.cfg.nvidia_model.clone()
    };
    if state.cfg.nvidia_free_models.contains(&candidate) {
        candidate
    } else {
        state.cfg.nvidia_free_models[0].clone()
    }
}

fn keychain_key() -> Option<String> {
    if let Ok(key) = env::var("NVIDIA_API_KEY") {
        if !key.is_empty() {
            return Some(key);
        }
    }
    let service = env::var("NVIDIA_KEYCHAIN_SERVICE").unwrap_or_else(|_| "NVIDIA_API_KEY".into());
    let account = env::var("NVIDIA_KEYCHAIN_ACCOUNT").ok();
    let mut args = vec!["find-generic-password", "-s", service.as_str()];
    if let Some(account) = account.as_deref() {
        args.extend(["-a", account]);
    }
    args.push("-w");
    Command::new("security")
        .args(args)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

fn difficulty(req: &ChatRequest) -> u8 {
    if let Some(Value::Object(extra)) = &req.extra_body {
        if let Some(v) = extra.get("difficulty").and_then(Value::as_u64) {
            return v.min(10) as u8;
        }
    }
    let text = req
        .messages
        .iter()
        .map(|m| m.content.as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let mut score = 0u8;
    if text.len() > 1200 {
        score += 1;
    }
    if text.len() > 4000 {
        score += 2;
    }
    for word in [
        "debug",
        "refactor",
        "architecture",
        "design",
        "optimize",
        "quantitative",
        "backtest",
        "strategy",
        "risk analysis",
        "compare",
        "trade-off",
        "step by step",
        "implement",
        "prove",
        "derive",
    ] {
        if text.contains(word) {
            score += 1;
        }
    }
    if text.contains("```") || text.contains("traceback") || text.contains("stack trace") {
        score += 2;
    }
    if text.matches('?').count() >= 3 || text.matches('\n').count() >= 20 {
        score += 1;
    }
    score.min(10)
}

fn requested_provider(req: &ChatRequest, headers: &HeaderMap, default: &str) -> String {
    if let Some(Value::Object(extra)) = &req.extra_body {
        if let Some(v) = extra.get("provider").and_then(Value::as_str) {
            return v.into();
        }
    }
    headers
        .get("x-ai-provider")
        .and_then(|v| v.to_str().ok())
        .unwrap_or(default)
        .into()
}

async fn local_healthy(state: &AppState) -> bool {
    state
        .http
        .get(format!("{}/models", state.cfg.local_url))
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

async fn nvidia_healthy(state: &AppState) -> bool {
    {
        let cache = state.nvidia_health.lock().await;
        if let Some(checked_at) = cache.checked_at {
            if checked_at.elapsed() < std::time::Duration::from_secs(15) {
                return cache.ok;
            }
        }
    }
    let Some(key) = keychain_key() else {
        return false;
    };
    let ok = state
        .http
        .get(format!("{}/models", state.cfg.nvidia_url))
        .bearer_auth(key)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false);
    let mut cache = state.nvidia_health.lock().await;
    cache.ok = ok;
    cache.checked_at = Some(Instant::now());
    ok
}

async fn apple_status(state: &AppState) -> Option<Value> {
    let output = timeout(
        std::time::Duration::from_secs(5),
        TokioCommand::new(&state.cfg.apple_bin)
            .arg("--status")
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    serde_json::from_slice(&output.stdout).ok()
}

async fn apple_healthy(state: &AppState) -> bool {
    apple_status(state)
        .await
        .and_then(|status| status.get("ok").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn github_status() -> Value {
    let root = env::var("THOMAS_REPO_ROOT")
        .unwrap_or_else(|_| "/Users/local.local/Projects/TradingSystem".into());
    let remote = Command::new("git")
        .args(["-C", &root, "remote", "get-url", "origin"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    let authenticated = Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    json!({"repo_root":root,"origin":remote,"gh_authenticated":authenticated,"token_exposure":"never returned"})
}

fn workspace_context(req: &ChatRequest) -> Option<String> {
    let text = req
        .messages
        .iter()
        .filter_map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let asks_for_files = [
        "inspect",
        "what is in",
        "list files",
        "projects folder",
        "project folder",
        "repository",
        "repo",
        "readme",
        "what does it do",
        "what's in",
    ]
    .iter()
    .any(|phrase| text.contains(phrase));
    if !asks_for_files {
        return None;
    }

    let root = env::var("THOMAS_REPO_ROOT")
        .unwrap_or_else(|_| "/Users/local.local/Projects/TradingSystem".into());
    let projects = std::path::Path::new(&root)
        .parent()
        .unwrap_or_else(|| std::path::Path::new(&root));
    let mut context = format!(
        "Thomas AI Full Access workspace context. The user asked about local files.\nRepository root: {root}\nProjects directory entries:\n"
    );
    if let Ok(entries) = fs::read_dir(projects) {
        let mut names = entries
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| !name.starts_with('.'))
            .collect::<Vec<_>>();
        names.sort();
        context.push_str(
            &names
                .into_iter()
                .take(100)
                .map(|name| format!("- {name}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    context.push_str("\n\nRepository files (tracked, excluding secrets):\n");
    if let Ok(output) = Command::new("git").args(["-C", &root, "ls-files"]).output() {
        if output.status.success() {
            let files = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|file| {
                    !file.starts_with('.') && !file.contains(".env") && !file.ends_with(".key")
                })
                .take(200)
                .map(|file| format!("- {file}"))
                .collect::<Vec<_>>();
            context.push_str(&files.join("\n"));
        }
    }
    let readme = std::path::Path::new(&root).join("README.md");
    if let Ok(contents) = fs::read_to_string(readme) {
        context.push_str("\n\nREADME excerpt:\n");
        context.push_str(&contents.chars().take(8000).collect::<String>());
    }
    context.push_str("\n\nUse this context to answer the user's file/repository question. Do not claim lack of access. Do not expose secrets or credentials.");
    Some(context)
}

fn agent_enabled(req: &ChatRequest) -> bool {
    matches!(&req.extra_body, Some(Value::Object(extra)) if extra.get("agent").and_then(Value::as_bool).unwrap_or(false))
}

fn agent_tools() -> Value {
    json!([
        {"type":"function","function":{"name":"list_directory","description":"List a local directory. Use for inspecting projects and files.","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}},
        {"type":"function","function":{"name":"read_file","description":"Read a UTF-8 text file from the local Mac.","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}},
        {"type":"function","function":{"name":"write_file","description":"Write or update a UTF-8 text file on the local Mac.","parameters":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}}},
        {"type":"function","function":{"name":"run_command","description":"Run a shell command as the logged-in user in the TradingSystem workspace.","parameters":{"type":"object","properties":{"command":{"type":"string"},"cwd":{"type":"string"}},"required":["command"]}}},
        {"type":"function","function":{"name":"git_status","description":"Inspect Git status and the configured GitHub remote.","parameters":{"type":"object","properties":{}}}}
    ])
}

fn allowed_path(path: &str) -> std::path::PathBuf {
    let root = env::var("THOMAS_REPO_ROOT")
        .unwrap_or_else(|_| "/Users/local.local/Projects/TradingSystem".into());
    let requested = std::path::Path::new(path);
    if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        std::path::Path::new(&root).join(requested)
    }
}

fn execute_tool_sync(name: &str, args: &Value) -> String {
    let root = env::var("THOMAS_REPO_ROOT")
        .unwrap_or_else(|_| "/Users/local.local/Projects/TradingSystem".into());
    match name {
        "list_directory" => {
            let path = allowed_path(args.get("path").and_then(Value::as_str).unwrap_or("."));
            match fs::read_dir(&path) {
                Ok(entries) => {
                    let mut names = entries
                        .flatten()
                        .filter_map(|entry| entry.file_name().into_string().ok())
                        .filter(|name| {
                            !name.starts_with('.') && name != "quant_dashboard.sqlite3-wal"
                        })
                        .collect::<Vec<_>>();
                    names.sort();
                    format!("{}\n{}", path.display(), names.join("\n"))
                }
                Err(error) => format!("list_directory error: {error}"),
            }
        }
        "read_file" => {
            let path = allowed_path(args.get("path").and_then(Value::as_str).unwrap_or(""));
            match fs::read_to_string(&path) {
                Ok(contents) => contents.chars().take(30000).collect(),
                Err(error) => format!("read_file error: {error}"),
            }
        }
        "write_file" => {
            let path = allowed_path(args.get("path").and_then(Value::as_str).unwrap_or(""));
            let content = args.get("content").and_then(Value::as_str).unwrap_or("");
            if path.extension().and_then(|ext| ext.to_str()) == Some("key")
                || path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.contains(".env"))
                    .unwrap_or(false)
            {
                return "write_file denied for credential-like files".into();
            }
            match fs::write(&path, content) {
                Ok(()) => format!("wrote {} bytes to {}", content.len(), path.display()),
                Err(error) => format!("write_file error: {error}"),
            }
        }
        "run_command" => "run_command is handled asynchronously".into(),
        "git_status" => {
            let status = Command::new("git")
                .args(["-C", &root, "status", "--short", "--branch"])
                .output();
            let remote = Command::new("git")
                .args(["-C", &root, "remote", "-v"])
                .output();
            format!(
                "status:\n{}\nremote:\n{}",
                status
                    .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                    .unwrap_or_default(),
                remote
                    .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                    .unwrap_or_default()
            )
        }
        _ => format!("unknown tool: {name}"),
    }
}

async fn read_limited<R: AsyncRead + Unpin>(mut reader: R, limit: usize) -> Vec<u8> {
    let mut kept = Vec::with_capacity(limit.min(8192));
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                if kept.len() < limit {
                    let take = (limit - kept.len()).min(count);
                    kept.extend_from_slice(&buffer[..take]);
                }
            }
        }
    }
    kept
}

async fn execute_tool(name: &str, args: &Value) -> String {
    if name != "run_command" {
        return execute_tool_sync(name, args);
    }
    let root = env::var("THOMAS_REPO_ROOT")
        .unwrap_or_else(|_| "/Users/local.local/Projects/TradingSystem".into());
    let command = args.get("command").and_then(Value::as_str).unwrap_or("");
    let cwd = args
        .get("cwd")
        .and_then(Value::as_str)
        .map(allowed_path)
        .unwrap_or_else(|| std::path::Path::new(&root).to_path_buf());
    let seconds = env::var("THOMAS_TOOL_TIMEOUT")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(120);
    let child = TokioCommand::new("/bin/zsh")
        .args(["-lc", command])
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn();
    let Ok(mut child) = child else {
        return "run_command error: unable to start shell".into();
    };
    let stdout = child
        .stdout
        .take()
        .map(|stream| tokio::spawn(read_limited(stream, 20_000)));
    let stderr = child
        .stderr
        .take()
        .map(|stream| tokio::spawn(read_limited(stream, 10_000)));
    let waited = timeout(std::time::Duration::from_secs(seconds), child.wait()).await;
    let output = match waited {
        Ok(Ok(status)) => {
            let out = match stdout {
                Some(task) => task.await.unwrap_or_default(),
                None => Vec::new(),
            };
            let err = match stderr {
                Some(task) => task.await.unwrap_or_default(),
                None => Vec::new(),
            };
            format!(
                "exit={}\nstdout:\n{}\nstderr:\n{}",
                status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out),
                String::from_utf8_lossy(&err)
            )
        }
        Ok(Err(error)) => format!("run_command error: {error}"),
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            let _ = stdout.map(|task| task.abort());
            let _ = stderr.map(|task| task.abort());
            format!("run_command timed out after {seconds}s")
        }
    };
    output
}

async fn call_upstream(
    state: &AppState,
    provider: &str,
    mut req: Value,
    model_override: Option<&str>,
) -> Result<Value, String> {
    if provider == "apple" {
        return call_apple(state, req).await;
    }
    let (base, configured_model, key) = if provider == "local" {
        (&state.cfg.local_url, &state.cfg.local_model, None)
    } else {
        (
            &state.cfg.nvidia_url,
            &state.cfg.nvidia_model,
            keychain_key(),
        )
    };
    let model = model_override.unwrap_or(configured_model);
    if let Value::Object(ref mut body) = req {
        body.remove("extra_body");
        let requested_model = body.get("model").and_then(Value::as_str);
        let use_model = if provider == "nvidia" {
            model_override
                .or(requested_model)
                .filter(|requested| {
                    state
                        .cfg
                        .nvidia_free_models
                        .iter()
                        .any(|free| free == requested)
                })
                .unwrap_or(model)
        } else {
            model_override
                .or(requested_model)
                .filter(|requested| *requested != "auto" && !requested.is_empty())
                .unwrap_or(model)
        };
        if provider != "nvidia" || use_model != requested_model.unwrap_or("") {
            body.insert("model".into(), json!(use_model));
        }
        body.insert("stream".into(), json!(false));
    }
    let mut request = state
        .http
        .post(format!("{}/chat/completions", base))
        .json(&req)
        .timeout(std::time::Duration::from_secs(if provider == "nvidia" {
            env::var("NVIDIA_REQUEST_TIMEOUT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(45)
        } else {
            env::var("MLX_REQUEST_TIMEOUT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(120)
        }));
    if let Some(secret) = key {
        request = request.bearer_auth(secret);
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("{}: {}", status, text));
    }
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

async fn call_apple(state: &AppState, req: Value) -> Result<Value, String> {
    let prompt = req
        .get("messages")
        .and_then(Value::as_array)
        .map(|messages| {
            messages
                .iter()
                .filter_map(|message| {
                    let role = message
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("user");
                    let content = message.get("content").and_then(Value::as_str)?;
                    Some(format!("{role}: {content}"))
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let request = json!({"prompt":prompt,"mode":state.cfg.apple_mode,"instructions":"You are Thomas AI's Apple-native application-development assistant. Give concise, technically useful answers. Do not claim access to local tools unless they are explicitly provided by the gateway."});
    let mut child = TokioCommand::new(&state.cfg.apple_bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("Apple AI bridge unavailable: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(format!("{}\n", request).as_bytes())
            .await
            .map_err(|error| format!("Apple AI bridge input failed: {error}"))?;
    }
    let output = timeout(
        std::time::Duration::from_secs(120),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| "Apple AI request timed out".to_string())?
    .map_err(|error| format!("Apple AI bridge failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Apple AI bridge exited unsuccessfully: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let response: Value =
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    if !response.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Err(response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Apple AI request failed")
            .into());
    }
    let text = response
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(
        json!({"id":"apple-chat-completion","object":"chat.completion","model":response.get("provider").and_then(Value::as_str).unwrap_or("apple-foundation-model"),"choices":[{"index":0,"message":{"role":"assistant","content":text},"finish_reason":"stop"}]}),
    )
}

async fn stream_upstream(
    state: &AppState,
    provider: &str,
    mut req: Value,
    model_override: Option<&str>,
) -> Result<Response, String> {
    if provider == "apple" {
        return Err(
            "Apple Foundation Models streaming bridge is not enabled for this request".into(),
        );
    }
    let (base, configured_model, key) = if provider == "local" {
        (&state.cfg.local_url, &state.cfg.local_model, None)
    } else {
        (
            &state.cfg.nvidia_url,
            &state.cfg.nvidia_model,
            keychain_key(),
        )
    };
    let model = model_override.unwrap_or(configured_model);
    if let Value::Object(ref mut body) = req {
        body.remove("extra_body");
        body.remove("tools");
        body.remove("tool_choice");
        let requested_model = body.get("model").and_then(Value::as_str);
        let use_model = if provider == "nvidia" {
            model_override
                .or(requested_model)
                .filter(|requested| {
                    state
                        .cfg
                        .nvidia_free_models
                        .iter()
                        .any(|free| free == requested)
                })
                .unwrap_or(model)
        } else {
            model_override
                .or(requested_model)
                .filter(|requested| *requested != "auto" && !requested.is_empty())
                .unwrap_or(model)
        };
        body.insert("model".into(), json!(use_model));
        body.insert("stream".into(), json!(true));
    }
    let mut request = state
        .http
        .post(format!("{}/chat/completions", base))
        .json(&req)
        .timeout(std::time::Duration::from_secs(if provider == "nvidia" {
            env::var("NVIDIA_REQUEST_TIMEOUT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(45)
        } else {
            env::var("MLX_REQUEST_TIMEOUT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(120)
        }));
    if let Some(secret) = key {
        request = request.bearer_auth(secret);
    }
    let response = request.send().await.map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.map_err(|error| error.to_string())?;
        return Err(format!("{}: {}", status, text));
    }
    let stream = response
        .bytes_stream()
        .map(|chunk| chunk.map_err(std::io::Error::other));
    Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("x-accel-buffering", "no")
        .body(axum::body::Body::from_stream(stream))
        .map_err(|error| error.to_string())
}

async fn agent_call(state: &AppState, provider: &str, mut payload: Value) -> Result<Value, String> {
    if let Value::Object(ref mut body) = payload {
        body.insert("tools".into(), agent_tools());
        body.insert("tool_choice".into(), json!("auto"));
        if let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) {
            messages.insert(0, json!({"role":"system","content":"You are Thomas AI's local-computer agent. You have access to tools that execute on the user's Mac. When the user asks you to inspect files, edit code, run commands, test software, or inspect Git, you MUST call the relevant local tool before answering. Do not claim you lack access. Tool results are local and must be used to produce the final answer. Use read/list tools before modifying files when practical, and describe completed actions concisely."}));
        }
    }
    for _ in 0..6 {
        let response = call_upstream(state, provider, payload.clone(), None).await?;
        let message = response
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        let calls = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if calls.is_empty() {
            return Ok(response);
        }
        if let Some(messages) = payload.get_mut("messages").and_then(Value::as_array_mut) {
            messages.push(message);
            for call in calls {
                let name = call
                    .pointer("/function/name")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let id = call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("tool-call");
                let arguments = call
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .and_then(|value| serde_json::from_str::<Value>(value).ok())
                    .unwrap_or_else(|| json!({}));
                let result = execute_tool(name, &arguments).await;
                messages.push(json!({"role":"tool","tool_call_id":id,"content":result}));
            }
        }
    }
    Err("agent tool loop exceeded four rounds".into())
}

async fn dispatch_call(
    state: &AppState,
    provider: &str,
    payload: Value,
    agent: bool,
) -> Result<Value, String> {
    let result = if agent && provider == "nvidia" {
        agent_call(state, provider, payload.clone()).await
    } else {
        call_upstream(state, provider, payload.clone(), None).await
    };
    if result.is_err() && provider == "nvidia" {
        let fallback = env::var("NVIDIA_FALLBACK_MODEL")
            .unwrap_or_else(|_| "deepseek-ai/deepseek-v4.1-flash".into());
        if fallback != state.cfg.nvidia_model {
            let mut fallback_payload = payload;
            if let Value::Object(ref mut body) = fallback_payload {
                body.insert("model".into(), json!(fallback));
            }
            return if agent {
                agent_call(state, provider, fallback_payload).await
            } else {
                call_upstream(state, provider, fallback_payload, Some(&fallback)).await
            };
        }
    }
    result
}

async fn dispatch_stream(
    state: &AppState,
    provider: &str,
    payload: Value,
) -> Result<Response, String> {
    let result = stream_upstream(state, provider, payload.clone(), None).await;
    if result.is_err() && provider == "nvidia" {
        let fallback = env::var("NVIDIA_FALLBACK_MODEL")
            .unwrap_or_else(|_| "nvidia/nemotron-3.5-bolt-e4b".into());
        let mut fallback_payload = payload;
        if let Value::Object(ref mut body) = fallback_payload {
            body.insert("model".into(), json!(fallback));
        }
        return stream_upstream(state, provider, fallback_payload, Some(&fallback)).await;
    }
    result
}

async fn chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ChatRequest>,
) -> Result<Response, (axum::http::StatusCode, Json<Value>)> {
    let selected = requested_provider(&req, &headers, &state.cfg.provider);
    let agent = agent_enabled(&req);
    let score = difficulty(&req);
    let preferred = if agent {
        // The bundled MLX model is excellent for local inference but does not
        // reliably emit tool calls. Use the verified free NVIDIA Nemotron
        // planner for agent requests; all tools still execute locally here.
        "nvidia"
    } else if selected == "auto" {
        let local_load = state
            .local_inflight
            .load(std::sync::atomic::Ordering::Relaxed);
        let nvidia_load = state
            .nvidia_inflight
            .load(std::sync::atomic::Ordering::Relaxed);
        let learned = state
            .router_model
            .as_ref()
            .and_then(|model| learned_provider(model, &req))
            .filter(|prediction| {
                let minimum = env::var("THOMAS_ROUTER_MIN_CONFIDENCE")
                    .ok()
                    .and_then(|value| value.parse::<f64>().ok())
                    .unwrap_or(0.65);
                prediction.confidence >= minimum
            });
        match learned
            .as_ref()
            .map(|prediction| prediction.provider.as_str())
        {
            Some("apple") => "apple",
            Some("nvidia") => "nvidia",
            Some("local") => "local",
            _ if score <= state.cfg.local_max_difficulty && local_load <= nvidia_load + 1 => {
                "local"
            }
            _ => "nvidia",
        }
    } else {
        selected.as_str()
    };
    let load_counter = if preferred == "local" {
        Arc::clone(&state.local_inflight)
    } else if preferred == "apple" {
        Arc::clone(&state.apple_inflight)
    } else {
        Arc::clone(&state.nvidia_inflight)
    };
    load_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let _load_guard = InflightGuard {
        counter: load_counter,
    };
    let alternate = if preferred == "local" {
        "nvidia"
    } else {
        "local"
    };
    let mut payload = serde_json::to_value(&req).unwrap_or_else(|_| json!({}));
    if let Some(context) = workspace_context(&req) {
        if let Some(messages) = payload.get_mut("messages").and_then(Value::as_array_mut) {
            messages.insert(0, json!({"role":"system","content":context}));
        }
    }
    if preferred == "nvidia" {
        if let Value::Object(ref mut body) = payload {
            body.insert("model".into(), json!(routed_nvidia_model(&state, score)));
        }
    }
    if req.stream {
        if agent {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                Json(
                    json!({"error":{"message":"streaming is unavailable for agent/tool-loop requests; disable agent for streaming"}}),
                ),
            ));
        }
        let started = Instant::now();
        let result = match preferred {
            "local" if local_healthy(&state).await => {
                dispatch_stream(&state, "local", payload.clone()).await
            }
            "nvidia" if nvidia_healthy(&state).await => {
                dispatch_stream(&state, "nvidia", payload.clone()).await
            }
            _ => Err("selected provider is unavailable".into()),
        };
        let response = match result {
            Ok(response) => response,
            Err(first) => {
                let alternate = if preferred == "local" {
                    "nvidia"
                } else {
                    "local"
                };
                let can_try = alternate == "local" && local_healthy(&state).await
                    || alternate == "nvidia" && nvidia_healthy(&state).await;
                if !can_try {
                    return Err((
                        axum::http::StatusCode::BAD_GATEWAY,
                        Json(json!({"error":{"message":first}})),
                    ));
                }
                dispatch_stream(&state, alternate, payload)
                    .await
                    .map_err(|error| {
                        (
                            axum::http::StatusCode::BAD_GATEWAY,
                            Json(json!({"error":{"message":error}})),
                        )
                    })?
            }
        };
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        let _ = elapsed;
        return Ok(response);
    }
    let started = Instant::now();
    let result = match preferred {
        "local" => {
            if local_healthy(&state).await {
                dispatch_call(&state, "local", payload.clone(), agent).await
            } else {
                Err("MLX is unavailable".into())
            }
        }
        "nvidia" => {
            if nvidia_healthy(&state).await {
                dispatch_call(&state, "nvidia", payload.clone(), agent).await
            } else {
                Err("NVIDIA_API_KEY is not configured".into())
            }
        }
        "apple" => {
            if apple_healthy(&state).await {
                dispatch_call(&state, "apple", payload.clone(), agent).await
            } else {
                Err("Apple Foundation Models are unavailable".into())
            }
        }
        _ => Err("provider must be local, nvidia, apple, or auto".into()),
    };
    if let Err(error) = &result {
        let mut stats = state.stats.lock().await;
        if preferred == "local" {
            stats.local_failures += 1;
        } else {
            stats.nvidia_failures += 1;
            if error.to_lowercase().contains("timeout") {
                stats.nvidia_timeouts += 1;
            }
        }
    }
    let (mut response, used) = match result {
        Ok(value) => (value, preferred),
        Err(first) => {
            let can_try = alternate == "local" && local_healthy(&state).await
                || alternate == "nvidia" && nvidia_healthy(&state).await
                || alternate == "apple" && apple_healthy(&state).await;
            if !can_try {
                return Err((
                    axum::http::StatusCode::BAD_GATEWAY,
                    Json(json!({"error":{"message":first}})),
                ));
            }
            (
                dispatch_call(&state, alternate, payload, agent)
                    .await
                    .map_err(|e| {
                        (
                            axum::http::StatusCode::BAD_GATEWAY,
                            Json(json!({"error":{"message":e}})),
                        )
                    })?,
                alternate,
            )
        }
    };
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    let mut stats = state.stats.lock().await;
    if used == "local" {
        stats.local_requests += 1;
        stats.local_latency_ms = elapsed;
    } else {
        stats.nvidia_requests += 1;
        stats.nvidia_latency_ms = elapsed;
    }
    drop(stats);
    if let Value::Object(ref mut object) = response {
        object.insert(
            "_thomas_ai".into(),
            json!({"provider":used,"difficulty":score,"latency_ms":elapsed.round()}),
        );
    }
    Ok(Json(response).into_response())
}

async fn models(State(state): State<AppState>) -> Json<Value> {
    let mut data = Vec::new();
    if let Ok(response) = state
        .http
        .get(format!("{}/models", state.cfg.local_url))
        .send()
        .await
    {
        if let Ok(value) = response.json::<Value>().await {
            if let Some(items) = value.get("data").and_then(Value::as_array) {
                data.extend(items.clone());
            }
        }
    }
    for model in &state.cfg.nvidia_free_models {
        data.push(json!({"id":model,"object":"model","owned_by":"nvidia-free-endpoint"}));
    }
    Json(json!({"object":"list","data":data}))
}

async fn status(State(state): State<AppState>) -> Json<Value> {
    let stats = state.stats.lock().await;
    Json(
        json!({"name":"Thomas AI","access_mode":"full","access_scope":"local macOS user","repository":github_status(),"routing":"difficulty_and_load","learned_router":state.router_model.is_some(),"local_max_difficulty":state.cfg.local_max_difficulty,"backends":{"mlx":{"url":state.cfg.local_url,"healthy":local_healthy(&state).await,"model":state.cfg.local_model,"inflight":state.local_inflight.load(std::sync::atomic::Ordering::Relaxed)},"apple":{"healthy":apple_healthy(&state).await,"mode":state.cfg.apple_mode,"bridge":state.cfg.apple_bin,"inflight":state.apple_inflight.load(std::sync::atomic::Ordering::Relaxed)},"nvidia":{"url":state.cfg.nvidia_url,"configured":nvidia_healthy(&state).await,"model":state.cfg.nvidia_model,"fast_model":state.cfg.nvidia_fast_model,"hard_model":state.cfg.nvidia_hard_model,"free_models":state.cfg.nvidia_free_models,"inflight":state.nvidia_inflight.load(std::sync::atomic::Ordering::Relaxed)}},"exo":{"url":env::var("EXO_BASE_URL").unwrap_or_default(),"rdma":"MLX JACCL over Thunderbolt 5"},"stats":&*stats}),
    )
}

async fn metrics(State(state): State<AppState>) -> Response {
    let stats = state.stats.lock().await;
    let body = format!(
        "# TYPE thomas_requests_total counter\nthomas_requests_total{{provider=\"local\"}} {}\nthomas_requests_total{{provider=\"nvidia\"}} {}\n# TYPE thomas_failures_total counter\nthomas_failures_total{{provider=\"local\"}} {}\nthomas_failures_total{{provider=\"nvidia\"}} {}\n# TYPE thomas_timeouts_total counter\nthomas_timeouts_total{{provider=\"nvidia\"}} {}\n# TYPE thomas_inflight gauge\nthomas_inflight{{provider=\"local\"}} {}\nthomas_inflight{{provider=\"nvidia\"}} {}\n# TYPE thomas_last_latency_ms gauge\nthomas_last_latency_ms{{provider=\"local\"}} {}\nthomas_last_latency_ms{{provider=\"nvidia\"}} {}\n",
        stats.local_requests,
        stats.nvidia_requests,
        stats.local_failures,
        stats.nvidia_failures,
        stats.nvidia_timeouts,
        state.local_inflight.load(std::sync::atomic::Ordering::Relaxed),
        state.nvidia_inflight.load(std::sync::atomic::Ordering::Relaxed),
        stats.local_latency_ms,
        stats.nvidia_latency_ms,
    );
    Response::builder()
        .header(header::CONTENT_TYPE, "text/plain; version=0.0.4")
        .body(axum::body::Body::from(body))
        .unwrap_or_else(|_| Response::new(axum::body::Body::empty()))
}

async fn serve(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let router_model = load_router_model();
    let state = AppState {
        cfg: Arc::new(config(cli.provider.clone())),
        router_model,
        http: Client::builder().pool_max_idle_per_host(32).build()?,
        stats: Arc::new(Mutex::new(Stats::default())),
        local_inflight: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        nvidia_inflight: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        apple_inflight: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        nvidia_health: Arc::new(Mutex::new(HealthCache::default())),
    };
    let app = Router::new()
        .route("/", get(|| async { Html(DASHBOARD) }))
        .route(
            "/health",
            get(|| async { Json(json!({"ok":true,"service":"Thomas AI"})) }),
        )
        .route("/api/status", get(status))
        .route("/metrics", get(metrics))
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(format!("{}:{}", cli.host, cli.port)).await?;
    println!("Thomas AI listening on http://{}:{}", cli.host, cli.port);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match &cli.command {
        None | Some(Commands::Serve) => serve(&cli).await?,
        Some(Commands::Status) => println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"service":"Thomas AI","gateway":"http://127.0.0.1:8788","mlx":"http://127.0.0.1:8080/v1","nvidia":"https://integrate.api.nvidia.com/v1","rdma":"MLX JACCL over Thunderbolt 5"})
            )?
        ),
        Some(Commands::Models) => {
            println!("Run `thomas-ai serve` then query http://127.0.0.1:8788/v1/models")
        }
        Some(Commands::Chat {
            prompt,
            provider,
            difficulty,
        }) => {
            let mut body = json!({"model":"auto","messages":[{"role":"user","content":prompt.join(" ")}],"extra_body":{"provider":provider}});
            if let Some(score) = difficulty {
                body["extra_body"]["difficulty"] = json!(score);
            }
            let gateway_url = env::var("AI_GATEWAY_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8788".into())
                .trim_end_matches('/')
                .to_string();
            let response = Client::new()
                .post(format!("{gateway_url}/v1/chat/completions"))
                .json(&body)
                .send()
                .await?
                .json::<Value>()
                .await?;
            println!(
                "{}",
                response
                    .get("choices")
                    .and_then(|v| v.get(0))
                    .and_then(|v| v.get("message"))
                    .and_then(|v| v.get("content"))
                    .and_then(Value::as_str)
                    .unwrap_or(&response.to_string())
            );
        }
        Some(Commands::MlxStart { model, port }) => {
            let status = Command::new("python3")
                .args([
                    "-m",
                    "mlx_lm.server",
                    "--model",
                    model,
                    "--port",
                    &port.to_string(),
                ])
                .status()?;
            if !status.success() {
                return Err("MLX server exited unsuccessfully".into());
            }
        }
        Some(Commands::Cluster { action }) => println!(
            "Thomas AI cluster {}: configure MLX JACCL/RDMA over Thunderbolt 5; EXO_BASE_URL={}",
            action,
            env::var("EXO_BASE_URL").unwrap_or_else(|_| "unset".into())
        ),
        Some(Commands::Exec { command }) => {
            if command.is_empty() {
                return Err("provide a command to execute".into());
            }
            println!(
                "{}",
                execute_tool("run_command", &json!({"command": command.join(" ")})).await
            );
        }
    }
    Ok(())
}
