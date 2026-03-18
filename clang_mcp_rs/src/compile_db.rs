/// Compile-command extraction from a compilation database.
///
/// Reads `compile_commands.json` via libclang's compilation database API
/// and filters out flags that are irrelevant for semantic analysis.
use std::collections::HashSet;
use std::path::Path;

use crate::clang_wrapper::{CompilationDatabase, norm};
#[cfg(test)]
use crate::clang_wrapper::{Index, TranslationUnit};

/// Header file extensions that cannot appear as direct compilation units.
const HEADER_EXTENSIONS: &[&str] = &["h", "hpp", "hxx", "hh", "H"];

/// Returns `true` if the file path has a header extension.
pub fn is_header_file(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map_or(false, |ext| HEADER_EXTENSIONS.contains(&ext))
}

/// Extract cleaned compile arguments for `src` from the compilation database in `build_dir`.
pub fn compile_args(build_dir: &str, src: &str) -> anyhow::Result<Vec<String>> {
    let db = CompilationDatabase::from_directory(build_dir)?;
    let cmds = db.compile_commands(src)?;
    if cmds.len() == 0 {
        anyhow::bail!("no compile command for {src}");
    }
    let cmd = cmds.get(0);
    let dir = cmd.directory();
    let filename = cmd.filename();
    extract_args_from_cmd(&cmd, &filename, &dir)
}

/// For header files not listed in `compile_commands.json`, borrow compile
/// arguments from a related source file.  Strategy:
///   1. Look for a `.cpp`/`.cc`/`.cxx` file in the same directory.
///   2. Otherwise fall back to the first entry in the database.
pub fn header_compile_args(build_dir: &str, header: &str) -> anyhow::Result<Vec<String>> {
    let db = CompilationDatabase::from_directory(build_dir)?;
    let all = db.all_compile_commands()?;
    let n = all.len();
    if n == 0 {
        anyhow::bail!("compilation database is empty");
    }

    let header_dir = Path::new(header).parent().map(|p| norm(&p.to_string_lossy()));

    // Prefer a source file in the same directory as the header.
    let mut fallback_idx: Option<u32> = None;
    for i in 0..n {
        let cmd = all.get(i);
        let file = cmd.filename();
        let dir = cmd.directory();
        let resolved = Path::new(&dir).join(&file);
        let resolved_norm = norm(&resolved.to_string_lossy());
        let file_dir = Path::new(&resolved_norm).parent().map(|p| p.to_string_lossy().to_string());
        if header_dir.as_deref() == file_dir.as_deref() {
            // Same directory – use this entry's args.
            return extract_args_from_cmd(&cmd, &file, &dir);
        }
        if fallback_idx.is_none() {
            fallback_idx = Some(i);
        }
    }

    // Fall back to first entry.
    let cmd = all.get(fallback_idx.unwrap_or(0));
    extract_args_from_cmd(&cmd, &cmd.filename(), &cmd.directory())
}

/// Shared arg-extraction logic for a single compile command.
fn extract_args_from_cmd(
    cmd: &crate::clang_wrapper::CompileCommand,
    filename: &str,
    dir: &str,
) -> anyhow::Result<Vec<String>> {
    let all_args = cmd.arguments();
    let args = &all_args[1..];

    let paired: HashSet<&str> = [
        "-o", "-MF", "-MT", "-MQ", "-MJ", "-Xclang", "-imacros",
        "-isysroot", "-target", "--target", "-ivfsoverlay", "-x",
    ]
    .into_iter()
    .collect();

    let single: HashSet<&str> = [
        "-c", "-M", "-MM", "-MD", "-MMD", "-MP", "-Winvalid-pch", "--",
    ]
    .into_iter()
    .collect();

    let mut srcs: HashSet<String> = HashSet::new();
    let p = Path::new(dir).join(filename);
    srcs.insert(norm(&p.to_string_lossy()));

    let mut out = Vec::new();
    let mut skip = false;
    let prefixes_paired = ["-o", "-MF", "-MT", "-MQ", "-MJ"];

    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if single.contains(a.as_str()) {
            continue;
        }
        if paired.contains(a.as_str()) {
            skip = true;
            continue;
        }
        if prefixes_paired.iter().any(|p| a.starts_with(p) && !paired.contains(a.as_str())) {
            continue;
        }
        // Filter out --driver-mode and similar long options with values
        if a.starts_with("--driver-mode=") {
            continue;
        }
        if !a.starts_with('-') {
            let resolved = Path::new(dir).join(a);
            if srcs.contains(&norm(&resolved.to_string_lossy())) {
                continue;
            }
        }
        out.push(a.clone());
    }
    Ok(out)
}

/// Parse a translation unit from `src` using compile flags from the build dir.
#[cfg(test)]
pub fn parse(build_dir: &str, src: &str) -> anyhow::Result<(Index, TranslationUnit)> {
    let args = compile_args(build_dir, src)?;
    let idx = Index::new();
    let tu = idx.parse(src, &args)?;
    Ok((idx, tu))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_args() {
        // Simulate argument filtering logic
        let args = vec![
            "/usr/bin/clang++", "-g", "-std=c++20",
            "-o", "out.o", "-c", "/workspace/samples/cpp/functions.cpp",
        ];
        let paired: HashSet<&str> = ["-o"].into_iter().collect();
        let single: HashSet<&str> = ["-c"].into_iter().collect();
        let src_norm = norm("/workspace/samples/cpp/functions.cpp");
        let mut srcs = HashSet::new();
        srcs.insert(src_norm);

        let mut out = Vec::new();
        let mut skip = false;
        for a in &args[1..] {
            if skip { skip = false; continue; }
            if single.contains(a) { continue; }
            if paired.contains(a) { skip = true; continue; }
            if !a.starts_with('-') && srcs.contains(&norm(a)) { continue; }
            out.push(a.to_string());
        }
        assert_eq!(out, vec!["-g", "-std=c++20"]);
    }

    #[test]
    fn test_compile_args_from_real_db() {
        // Test against the actual compile_commands.json in /workspace/build
        let args = compile_args("/workspace/build", "/workspace/samples/cpp/functions.cpp")
            .expect("compile_args failed");
        // Should not contain -c, -o, or the source file itself
        for a in &args {
            assert_ne!(a, "-c");
            assert!(!a.starts_with("-o"), "found -o flag: {a}");
        }
        // Should contain a std flag
        assert!(args.iter().any(|a| a.starts_with("-std=")), "expected -std= flag");
    }

    #[test]
    fn test_compile_args_nonexistent_db() {
        let result = compile_args("/nonexistent/build", "/workspace/nonexistent.cpp");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_real_tu() {
        let (_, tu) = parse("/workspace/build", "/workspace/samples/cpp/functions.cpp")
            .expect("parse failed");
        let cursor = tu.cursor();
        assert!(!cursor.is_null());
    }

    #[test]
    fn test_is_header_file() {
        assert!(is_header_file("foo.h"));
        assert!(is_header_file("/some/path/bar.hpp"));
        assert!(is_header_file("baz.hxx"));
        assert!(is_header_file("qux.hh"));
        assert!(is_header_file("upper.H"));
        assert!(!is_header_file("main.cpp"));
        assert!(!is_header_file("main.c"));
        assert!(!is_header_file("Makefile"));
        assert!(!is_header_file("no_ext"));
    }

    #[test]
    fn test_header_compile_args_returns_flags() {
        // shapes.h lives in the same directory as functions.cpp / classes.cpp / data.cpp
        let args = header_compile_args("/workspace/build", "/workspace/samples/cpp/shapes.h")
            .expect("header_compile_args failed");
        // Should contain a std flag borrowed from a sibling source file
        assert!(args.iter().any(|a| a.starts_with("-std=")), "expected -std= flag, got {args:?}");
        // Should not contain -c, -o, --driver-mode, --, or -x
        for a in &args {
            assert_ne!(a, "-c", "found -c");
            assert_ne!(a, "--", "found --");
            assert!(!a.starts_with("-o"), "found -o flag: {a}");
            assert!(!a.starts_with("--driver-mode"), "found --driver-mode: {a}");
            assert_ne!(a, "-x", "found -x");
        }
    }

    #[test]
    fn test_header_compile_args_nonexistent_db() {
        let result = header_compile_args("/nonexistent/build", "/workspace/samples/cpp/shapes.h");
        assert!(result.is_err());
    }
}
