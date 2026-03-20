/// clang_mcp — Rust drop-in replacement for clang_mcp.py.
///
/// Provides semantic analysis of C++ source files using libclang,
/// exposing the tool command (cpp_semantic_query) and legacy commands
/// (list-functions, describe-function, doctor).
#[allow(non_upper_case_globals)]
mod clang_wrapper;
mod compile_db;
#[allow(non_upper_case_globals)]
mod index;
#[allow(non_upper_case_globals)]
mod symbols;
#[cfg(test)]
mod test_support;
mod tools;
mod types;

use std::fs;
use std::process;

use clap::{Parser, Subcommand};
use serde_json::Value;

use clang_wrapper::norm;

fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    process::exit(1);
}

#[derive(Parser)]
#[command(name = "clang_mcp")]
struct Cli {
    #[arg(long = "build-dir")]
    build_dir: Option<String>,

    #[arg(long = "file")]
    file: Option<String>,

    #[arg(long = "workspace-root")]
    workspace_root: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(name = "list-functions")]
    ListFunctions,

    #[command(name = "describe-function")]
    DescribeFunction {
        #[arg(long)]
        name: String,
    },

    #[command(name = "doctor")]
    Doctor,

    #[command(name = "cpp_semantic_query")]
    CppSemanticQuery {
        #[arg(long = "request-json")]
        request_json: Option<String>,
        #[arg(long = "request-file")]
        request_file: Option<String>,
    },
}

fn parse_request(
    request_json: &Option<String>,
    request_file: &Option<String>,
) -> serde_json::Map<String, Value> {
    if let Some(json_str) = request_json {
        match serde_json::from_str::<Value>(json_str) {
            Ok(Value::Object(m)) => return m,
            Ok(_) => die("invalid JSON in --request-json: expected object"),
            Err(e) => die(&format!("invalid JSON in --request-json: {e}")),
        }
    }
    if let Some(path) = request_file {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => die(&format!("request file not found: {path}")),
        };
        match serde_json::from_str::<Value>(&content) {
            Ok(Value::Object(m)) => return m,
            Ok(_) => die(&format!("invalid JSON in --request-file {path}: expected object")),
            Err(e) => die(&format!("invalid JSON in --request-file {path}: {e}")),
        }
    }
    die("one of --request-json or --request-file is required");
}

fn main() {
    let cli = Cli::parse();

    let command = match cli.command {
        Some(c) => c,
        None => {
            Cli::parse_from(["clang_mcp", "--help"]);
            return;
        }
    };

    let src = cli.file.as_deref().map(|f| norm(f));
    let build = cli.build_dir.as_deref().map(|b| norm(b));
    let ws_root = cli.workspace_root.as_deref().map(|w| norm(w));

    let output: Value = match command {
        Commands::Doctor => tools::doctor(build.as_deref(), src.as_deref()),

        Commands::CppSemanticQuery { request_json, request_file } => {
            let (b, s) = require_build_and_file(&build, &src, "cpp_* tool commands");
            run_tool(b, s, ws_root.as_deref(), "cpp_semantic_query", &request_json, &request_file)
        }

        Commands::ListFunctions => {
            let (b, s) = require_build_and_file(&build, &src, "list-functions and describe-function");
            let idx = index::load_index(b, s, ws_root.as_deref()).unwrap_or_else(|e| die(&e.to_string()));
            tools::list_functions(&idx)
        }
        Commands::DescribeFunction { name } => {
            let (b, s) = require_build_and_file(&build, &src, "list-functions and describe-function");
            let idx = index::load_index(b, s, ws_root.as_deref()).unwrap_or_else(|e| die(&e.to_string()));
            tools::describe_function(&idx, &name)
        }
    };

    let json_str = serde_json::to_string_pretty(&output).unwrap();
    println!("{json_str}");
}

fn require_build_and_file<'a>(build: &'a Option<String>, src: &'a Option<String>, context: &str) -> (&'a str, &'a str) {
    match (build.as_deref(), src.as_deref()) {
        (Some(b), Some(s)) => (b, s),
        _ => die(&format!("--build-dir and --file are required for {context}")),
    }
}

fn run_tool(
    build_dir: &str,
    src: &str,
    ws_root: Option<&str>,
    cmd_name: &str,
    request_json: &Option<String>,
    request_file: &Option<String>,
) -> Value {
    let req = parse_request(request_json, request_file);

    match index::load_index(build_dir, src, ws_root) {
        Ok(idx) => match cmd_name {
            "cpp_semantic_query" => tools::tool_cpp_semantic_query(&idx, &req),
            _ => die(&format!("unknown command: {cmd_name}")),
        },
        Err(e) => build_error_response(cmd_name, &e.to_string()),
    }
}

fn build_error_response(cmd_name: &str, message: &str) -> Value {
    let kind = match cmd_name {
        "cpp_semantic_query" => "list",
        _ => "list",
    };
    let mut out = types::error_base("INTERNAL_ERROR", message);
    out.insert("result_kind".to_string(), Value::String(kind.to_string()));
    out.insert("items".to_string(), Value::Array(vec![]));
    out.insert("page".to_string(), types::page_json(None, false, 0));
    Value::Object(out)
}
