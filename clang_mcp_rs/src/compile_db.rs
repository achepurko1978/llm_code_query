/// Compile-command extraction from a compilation database.
///
/// Reads `compile_commands.json` via libclang's compilation database API
/// and filters out flags that are irrelevant for semantic analysis.
use std::collections::HashSet;
use std::path::Path;

use crate::clang_wrapper::{CompilationDatabase, norm};
#[cfg(test)]
use crate::clang_wrapper::{Index, TranslationUnit};

/// Extract cleaned compile arguments for `src` from the compilation database in `build_dir`.
pub fn compile_args(build_dir: &str, src: &str) -> anyhow::Result<Vec<String>> {
    let db = CompilationDatabase::from_directory(build_dir)?;
    let cmds = db.compile_commands(src)?;
    if cmds.len() == 0 {
        anyhow::bail!("no compile command for {src}");
    }
    let cmd = cmds.get(0);
    let all_args = cmd.arguments();
    // Skip the compiler itself (first arg)
    let args = &all_args[1..];
    let dir = cmd.directory();
    let filename = cmd.filename();

    let paired: HashSet<&str> = [
        "-o", "-MF", "-MT", "-MQ", "-MJ", "-Xclang", "-imacros",
        "-isysroot", "-target", "--target", "-ivfsoverlay",
    ]
    .into_iter()
    .collect();

    let single: HashSet<&str> = [
        "-c", "-M", "-MM", "-MD", "-MMD", "-MP", "-Winvalid-pch",
    ]
    .into_iter()
    .collect();

    let mut srcs: HashSet<String> = HashSet::new();
    srcs.insert(norm(src));
    let p = Path::new(&dir).join(&filename);
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
        // Merged flags like -oFoo
        if prefixes_paired.iter().any(|p| a.starts_with(p) && !paired.contains(a.as_str())) {
            continue;
        }
        // Skip source files
        if !a.starts_with('-') {
            let resolved = Path::new(&dir).join(a);
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
}
