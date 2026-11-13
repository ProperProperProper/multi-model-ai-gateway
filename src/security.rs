use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

#[derive(Default)]
struct Baseline {
    count: u64,
    mean: f64,
    m2: f64,
}

impl Baseline {
    fn update(&mut self, value: f64) {
        self.count += 1;
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        self.m2 += delta * (value - self.mean);
    }
    fn z_score(&self, value: f64) -> f64 {
        if self.count < 20 {
            return 0.0;
        }
        let variance = self.m2 / (self.count - 1) as f64;
        if variance <= f64::EPSILON {
            0.0
        } else {
            (value - self.mean).abs() / variance.sqrt()
        }
    }
}

#[derive(Clone)]
pub struct SecurityGuard {
    mode: String,
    agent_mode: String,
    prompt_length: Arc<Mutex<Baseline>>,
    kv_norm: Arc<Mutex<Baseline>>,
    observed: Arc<AtomicU64>,
    blocked: Arc<AtomicU64>,
}

#[derive(Debug, serde::Serialize)]
pub struct Decision {
    pub allowed: bool,
    pub score: f64,
    pub reasons: Vec<String>,
    pub action: String,
}

#[derive(Debug, Deserialize)]
pub struct KvTelemetry {
    pub model: Option<String>,
    pub pages: f64,
    pub cache_tokens: f64,
    pub mean_norm: f64,
    pub max_norm: f64,
    pub attention_entropy: Option<f64>,
}

impl SecurityGuard {
    pub fn new() -> Self {
        Self {
            mode: std::env::var("MULTI_MODEL_GATEWAY_SECURITY_MODE")
                .unwrap_or_else(|_| "observe".into()),
            agent_mode: std::env::var("MULTI_MODEL_GATEWAY_SECURITY_AGENT_MODE")
                .unwrap_or_else(|_| "guard".into()),
            prompt_length: Arc::new(Mutex::new(Baseline::default())),
            kv_norm: Arc::new(Mutex::new(Baseline::default())),
            observed: Arc::new(AtomicU64::new(0)),
            blocked: Arc::new(AtomicU64::new(0)),
        }
    }

    fn decision(&self, score: f64, mut reasons: Vec<String>, agent: bool) -> Decision {
        let score = score.clamp(0.0, 1.0);
        let enforce = self.mode == "enforce" || (agent && self.agent_mode == "enforce");
        let guard = self.mode == "guard" || (agent && self.agent_mode == "guard");
        let high_signal = reasons.iter().any(|reason| {
            reason == "prompt-injection-pattern"
                || reason == "jailbreak-pattern"
                || reason == "destructive-action-pattern"
                || reason == "high-risk-command"
                || reason == "ontology-type-violation"
                || reason == "ontology-constraint-violation"
        });
        let allowed = !(enforce && (score >= 0.75 || high_signal))
            && !(guard && agent && (score >= 0.9 || high_signal));
        if reasons.is_empty() {
            reasons.push("normal-behavior".into());
        }
        self.observed.fetch_add(1, Ordering::Relaxed);
        if !allowed {
            self.blocked.fetch_add(1, Ordering::Relaxed);
        }
        Decision {
            allowed,
            score,
            reasons,
            action: if allowed { "allow" } else { "block" }.into(),
        }
    }

    pub fn inspect_request(&self, request: &Value, agent: bool) -> Decision {
        let text = request
            .get("messages")
            .and_then(Value::as_array)
            .map(|messages| {
                messages
                    .iter()
                    .filter_map(|message| message.get("content").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let lower = text.to_lowercase();
        let mut score = 0.0;
        let mut reasons = Vec::new();
        for (terms, weight, reason) in [
            (
                &[
                    "ignore previous",
                    "ignore all instructions",
                    "system prompt",
                ][..],
                0.45,
                "prompt-injection-pattern",
            ),
            (
                &["jailbreak", "developer mode", "do anything now"][..],
                0.45,
                "jailbreak-pattern",
            ),
            (
                &["api key", "password", "secret", "private key", "credential"][..],
                0.25,
                "sensitive-data-pattern",
            ),
            (
                &["rm -rf", "delete repository", "drop database", "force push"][..],
                0.55,
                "destructive-action-pattern",
            ),
        ] {
            if terms.iter().any(|term| lower.contains(term)) {
                score += weight;
                reasons.push(reason.into());
            }
        }
        if agent && lower.contains("run_command") {
            score += 0.1;
            reasons.push("agent-command-capability".into());
        }
        let length_z = self
            .prompt_length
            .lock()
            .ok()
            .map(|baseline| baseline.z_score(text.len() as f64))
            .unwrap_or(0.0);
        if length_z >= 4.0 {
            score += 0.2;
            reasons.push("prompt-length-anomaly".into());
        }
        if score < 0.35 {
            if let Ok(mut baseline) = self.prompt_length.lock() {
                baseline.update(text.len() as f64);
            }
        }
        self.decision(score, reasons, agent)
    }

    pub fn inspect_tool(&self, name: &str, args: &Value) -> Decision {
        let text = format!("{name} {args}").to_lowercase();
        let mut score = 0.0;
        let mut reasons = Vec::new();
        if let Some(reason) = Self::ontology_violation(name, args) {
            score = 1.0;
            reasons.push(reason.into());
        }
        if [
            "rm -rf",
            "git reset --hard",
            "git push --force",
            "curl ",
            "wget ",
            "fetch(",
            "python -c",
            "python3 -c",
            "base64 -d",
            "open('/",
            "security find-generic-password",
            "ssh-keygen",
            ".ssh/",
            ".env",
            "chmod 777",
        ]
        .iter()
        .any(|term| text.contains(term))
        {
            score += 0.9;
            reasons.push("high-risk-command".into());
        }
        if name == "write_file" {
            score += 0.1;
            reasons.push("file-write".into());
        }
        self.decision(score, reasons, true)
    }

    /// Typed logical guardrails outside the model. The model may propose a tool
    /// call, but it cannot invent fields, roles, providers, or argument types.
    fn ontology_violation(name: &str, args: &Value) -> Option<&'static str> {
        let Some(object) = args.as_object() else {
            return Some("ontology-type-violation");
        };
        let string_field = |key: &str| {
            object
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
        };
        match name {
            "list_directory" | "read_file" => string_field("path")
                .is_none()
                .then_some("ontology-type-violation"),
            "write_file" => {
                if string_field("path").is_none()
                    || object.get("content").and_then(Value::as_str).is_none()
                {
                    Some("ontology-type-violation")
                } else if string_field("path")
                    .is_some_and(|path| path.ends_with(".key") || path.contains(".env"))
                {
                    Some("ontology-constraint-violation")
                } else {
                    None
                }
            }
            "run_command" => {
                if string_field("command").is_none()
                    || object.get("cwd").is_some_and(|value| !value.is_string())
                {
                    Some("ontology-type-violation")
                } else {
                    None
                }
            }
            "git_status" => (!object.is_empty()).then_some("ontology-constraint-violation"),
            "spawn_subagent" => {
                let role = string_field("role");
                let task = string_field("task");
                let provider = object
                    .get("provider")
                    .and_then(Value::as_str)
                    .unwrap_or("nvidia");
                if role.is_none()
                    || task.is_none()
                    || !["research", "implementation", "code_review"].contains(&role.unwrap_or(""))
                    || !["local", "nvidia", "apple"].contains(&provider)
                {
                    Some("ontology-type-violation")
                } else if task.unwrap_or("").chars().count() > 12_000 {
                    Some("ontology-constraint-violation")
                } else {
                    None
                }
            }
            _ => Some("ontology-constraint-violation"),
        }
    }

    pub fn ingest_kv(&self, telemetry: &KvTelemetry) -> Decision {
        let mut score = 0.0;
        let mut reasons = Vec::new();
        if let Ok(baseline) = self.kv_norm.lock() {
            if baseline.z_score(telemetry.mean_norm) >= 4.0 {
                score += 0.7;
                reasons.push("kv-mean-norm-anomaly".into());
            }
        }
        if telemetry.max_norm > 100.0 {
            score += 0.3;
            reasons.push("kv-max-norm-outlier".into());
        }
        if telemetry
            .attention_entropy
            .is_some_and(|value| !value.is_finite())
        {
            score += 0.8;
            reasons.push("kv-invalid-entropy".into());
        }
        if score < 0.35 {
            if let Ok(mut baseline) = self.kv_norm.lock() {
                baseline.update(telemetry.mean_norm);
            }
        }
        self.decision(score, reasons, false)
    }

    pub fn status(&self) -> Value {
        json!({"mode":self.mode,"agent_mode":self.agent_mode,"observed":self.observed.load(Ordering::Relaxed),"blocked":self.blocked.load(Ordering::Relaxed),"raw_kv":"optional aggregate telemetry endpoint","privacy":"raw prompts and KV tensors are not persisted"})
    }
}
