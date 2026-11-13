use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub path: String,
    pub line: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct GraphIndex {
    pub root: String,
    pub generated_at: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

impl GraphIndex {
    pub fn build(root: &Path) -> Self {
        let mut graph = Self {
            root: root.display().to_string(),
            generated_at: format!("{}", chrono_like_now()),
            nodes: Vec::new(),
            edges: Vec::new(),
        };
        let mut files = Vec::new();
        collect_files(root, root, &mut files);
        let mut symbols: HashMap<String, String> = HashMap::new();
        for path in &files {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(path)
                .display()
                .to_string();
            let file_id = format!("file:{relative}");
            graph.nodes.push(GraphNode {
                id: file_id.clone(),
                kind: "file".into(),
                name: relative.clone(),
                path: relative.clone(),
                line: 1,
            });
            if let Ok(contents) = fs::read_to_string(path) {
                for (index, line) in contents.lines().enumerate() {
                    if let Some((kind, name)) = symbol(line) {
                        let id = format!("symbol:{relative}:{}:{name}", index + 1);
                        graph.nodes.push(GraphNode {
                            id: id.clone(),
                            kind: kind.into(),
                            name: name.clone(),
                            path: relative.clone(),
                            line: index + 1,
                        });
                        symbols.entry(name).or_insert(id.clone());
                        graph.edges.push(GraphEdge {
                            from: file_id.clone(),
                            to: id,
                            kind: "defines".into(),
                        });
                    }
                    if let Some(dependency) = import_name(line) {
                        let external_id = format!("external:{dependency}");
                        if !graph.nodes.iter().any(|node| node.id == external_id) {
                            graph.nodes.push(GraphNode {
                                id: external_id.clone(),
                                kind: "dependency".into(),
                                name: dependency.clone(),
                                path: String::new(),
                                line: 0,
                            });
                        }
                        graph.edges.push(GraphEdge {
                            from: file_id.clone(),
                            to: external_id,
                            kind: "imports".into(),
                        });
                    }
                }
            }
        }
        let known = symbols.clone();
        for path in &files {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(path)
                .display()
                .to_string();
            let file_id = format!("file:{relative}");
            if let Ok(contents) = fs::read_to_string(path) {
                for line in contents.lines() {
                    for (name, target) in &known {
                        if name.len() > 2
                            && line.contains(&format!("{name}("))
                            && !line.trim_start().starts_with(name)
                        {
                            graph.edges.push(GraphEdge {
                                from: file_id.clone(),
                                to: target.clone(),
                                kind: "calls".into(),
                            });
                        }
                    }
                }
            }
        }
        deduplicate(&mut graph.edges);
        graph
    }

    pub fn query(&self, query: &str, limit: usize) -> Value {
        let needle = query.to_lowercase();
        let nodes = self
            .nodes
            .iter()
            .filter(|node| {
                node.name.to_lowercase().contains(&needle)
                    || node.path.to_lowercase().contains(&needle)
            })
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        let ids = nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        let edges = self
            .edges
            .iter()
            .filter(|edge| ids.contains(&edge.from) || ids.contains(&edge.to))
            .take(limit * 4)
            .cloned()
            .collect::<Vec<_>>();
        json!({"query":query,"nodes":nodes,"edges":edges})
    }

    pub fn report(&self) -> String {
        let files = self.nodes.iter().filter(|node| node.kind == "file").count();
        let symbols = self
            .nodes
            .iter()
            .filter(|node| {
                node.kind == "function"
                    || node.kind == "class"
                    || node.kind == "struct"
                    || node.kind == "enum"
                    || node.kind == "trait"
                    || node.kind == "impl"
            })
            .count();
        let imports = self
            .edges
            .iter()
            .filter(|edge| edge.kind == "imports")
            .count();
        let calls = self
            .edges
            .iter()
            .filter(|edge| edge.kind == "calls")
            .count();
        format!("# Multi-Model AI Gateway local architecture graph\n\n- Root: `{}`\n- Files: {}\n- Symbols: {}\n- Import edges: {}\n- Call edges: {}\n\n## Most connected nodes\n\n{}", self.root, files, symbols, imports, calls, most_connected(self))
    }

    pub fn html(&self) -> String {
        let data = serde_json::to_string(self).unwrap_or_else(|_| "{}".into());
        format!(
            r#"<!doctype html><meta charset="utf-8"><title>Multi-Model AI Gateway Graph</title><style>body{{font:15px system-ui;background:#0b1110;color:#eef5ef;max-width:1200px;margin:30px auto;padding:0 20px}}input{{width:70%;padding:10px;background:#15221e;color:white;border:1px solid #49665a;border-radius:6px}}li{{margin:7px 0}}.file{{color:#78e0ae}}.symbol{{color:#ffdc65}}.dependency{{color:#9dbbff}}small{{color:#9aaca3}}</style><h1>Multi-Model AI Gateway architecture graph</h1><p><small>Local deterministic scan. No source is uploaded.</small></p><input id="q" placeholder="Filter nodes" oninput="draw()"><div id="summary"></div><ul id="nodes"></ul><script>const g={data};function draw(){{let q=document.getElementById('q').value.toLowerCase();let n=g.nodes.filter(x=>(x.name+' '+x.path).toLowerCase().includes(q)).slice(0,500);document.getElementById('summary').textContent=n.length+' matching nodes · '+g.edges.length+' edges';document.getElementById('nodes').innerHTML=n.map(x=>`<li class="${{x.kind}}"><b>${{x.kind}}</b> ${{x.name}} <small>${{x.path}}:${{x.line}}</small></li>`).join('')}}draw()</script>"#
        )
    }
}

fn collect_files(root: &Path, current: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if ![
                ".git",
                "target",
                ".build",
                ".venv",
                "node_modules",
                "__pycache__",
                "lab_runs",
            ]
            .contains(&name)
            {
                collect_files(root, &path, output);
            }
        } else if matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "toml" | "md" | "json" | "sql")
        ) {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if !name.contains(".env") && !name.ends_with(".key") && !name.ends_with(".pem") {
                output.push(path);
            }
        }
    }
}

fn symbol(line: &str) -> Option<(&'static str, String)> {
    let trimmed = line.trim_start();
    for (prefix, kind) in [
        ("fn ", "function"),
        ("pub fn ", "function"),
        ("async fn ", "function"),
        ("pub async fn ", "function"),
        ("struct ", "struct"),
        ("pub struct ", "struct"),
        ("enum ", "enum"),
        ("pub enum ", "enum"),
        ("trait ", "trait"),
        ("pub trait ", "trait"),
        ("class ", "class"),
        ("def ", "function"),
        ("async def ", "function"),
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let name = rest
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                return Some((kind, name.into()));
            }
        }
    }
    if let Some(rest) = trimmed.strip_prefix("impl ") {
        let name = rest
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or("");
        if !name.is_empty() {
            return Some(("impl", name.into()));
        }
    }
    None
}

fn import_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    for prefix in ["use ", "mod ", "import ", "from "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let name = rest
                .trim()
                .trim_end_matches(';')
                .split_whitespace()
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                return Some(name.trim_matches(|c: char| c == '{' || c == '"').into());
            }
        }
    }
    None
}

fn deduplicate(edges: &mut Vec<GraphEdge>) {
    let mut seen = HashSet::new();
    edges.retain(|edge| seen.insert(format!("{}|{}|{}", edge.from, edge.to, edge.kind)));
}

fn most_connected(graph: &GraphIndex) -> String {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for edge in &graph.edges {
        *counts.entry(&edge.from).or_default() += 1;
        *counts.entry(&edge.to).or_default() += 1;
    }
    let mut values = counts.into_iter().collect::<Vec<_>>();
    values.sort_by(|a, b| b.1.cmp(&a.1));
    values
        .into_iter()
        .take(10)
        .map(|(id, count)| format!("- `{id}` — {count} connections"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn chrono_like_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
