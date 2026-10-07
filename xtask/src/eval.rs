use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

pub fn run(root: &Path, answers: &str) -> Result<(), String> {
    let answer_directory = root
        .join(answers)
        .canonicalize()
        .map_err(|error| format!("answer directory {answers}: {error}"))?;
    let tasks_path = root.join("evals").join("tasks.json");
    let tasks: Value = serde_json::from_str(
        &std::fs::read_to_string(&tasks_path)
            .map_err(|error| format!("read {}: {error}", tasks_path.display()))?,
    )
    .map_err(|error| format!("decode tasks: {error}"))?;
    let tasks = tasks.as_array().ok_or("tasks.json must be an array")?;

    let answers: Vec<(String, PathBuf)> = tasks
        .iter()
        .filter_map(|task| task["id"].as_str())
        .map(|id| (id.to_owned(), answer_directory.join(format!("{id}.rs"))))
        .collect();
    let failing_files = compile_failures(root, &answers)?;

    let mut compile_score = 0;
    let mut semantic_score = 0;
    for task in tasks {
        let id = task["id"].as_str().ok_or("every task needs an id")?;
        let answer = answer_directory.join(format!("{id}.rs"));
        let Ok(source) = std::fs::read_to_string(&answer) else {
            println!("{id}: compile=fail semantic=fail (missing file)");
            continue;
        };
        let compiles = !failing_files.contains(&answer);
        let markers = |key: &str| -> Vec<String> {
            task[key]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        };
        let compact = without_whitespace(&source);
        let required = markers("required")
            .iter()
            .all(|marker| compact.contains(&without_whitespace(marker)));
        let forbidden = markers("forbidden")
            .iter()
            .all(|marker| !compact.contains(&without_whitespace(marker)));
        let uses_public_package =
            source.contains("openhandle::") && !source.contains("openhandle::__");
        let semantic = required && forbidden && uses_public_package;
        compile_score += usize::from(compiles);
        semantic_score += usize::from(semantic);
        println!(
            "{id}: compile={} semantic={}",
            if compiles { "pass" } else { "fail" },
            if semantic { "pass" } else { "fail" }
        );
    }
    println!(
        "Agent SDK eval: compile {compile_score}/{}, semantic {semantic_score}/{}",
        tasks.len(),
        tasks.len()
    );
    if compile_score == tasks.len() && semantic_score == tasks.len() {
        return Ok(());
    }
    Err("Agent SDK eval failed".to_owned())
}

fn compile_failures(
    root: &Path,
    answers: &[(String, PathBuf)],
) -> Result<BTreeSet<PathBuf>, String> {
    let crate_directory = root.join("target").join("agent-eval");
    std::fs::create_dir_all(crate_directory.join("src"))
        .map_err(|error| format!("create eval crate: {error}"))?;
    let manifest = format!(
        "[package]\nname = \"openhandle-agent-eval\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n\
         [dependencies]\nopenhandle = {{ path = {:?} }}\n\n[workspace]\n",
        root.display().to_string()
    );
    std::fs::write(crate_directory.join("Cargo.toml"), manifest)
        .map_err(|error| format!("write eval manifest: {error}"))?;
    let lockfile = root.join("Cargo.lock");
    if lockfile.exists() {
        std::fs::copy(&lockfile, crate_directory.join("Cargo.lock"))
            .map_err(|error| format!("copy lockfile: {error}"))?;
    }
    let mut library = String::from("#![allow(dead_code, unused)]\n\n");
    for (index, (_, path)) in answers.iter().enumerate() {
        if path.exists() {
            writeln!(
                library,
                "#[path = {:?}]\nmod task_{index};",
                path.display().to_string()
            )
            .unwrap();
        }
    }
    std::fs::write(crate_directory.join("src").join("lib.rs"), library)
        .map_err(|error| format!("write eval crate: {error}"))?;

    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()))
        .args(["check", "--quiet", "--message-format", "short"])
        .current_dir(&crate_directory)
        .output()
        .map_err(|error| format!("run cargo check: {error}"))?;
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    let mut failures = BTreeSet::new();
    for line in diagnostics.lines().filter(|line| line.contains(": error")) {
        if let Some((_, path)) = answers
            .iter()
            .find(|(_, path)| line.starts_with(&path.display().to_string()))
        {
            failures.insert(path.clone());
        }
    }
    if !output.status.success() {
        eprint!("{diagnostics}");
        if failures.is_empty() {
            return Err(
                "the eval crate failed to compile for a reason outside the answers".to_owned(),
            );
        }
    }
    Ok(failures)
}

fn without_whitespace(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}
