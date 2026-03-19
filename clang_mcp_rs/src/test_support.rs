#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use std::process::Command;
#[cfg(test)]
use std::sync::OnceLock;

#[cfg(test)]
pub const TEST_BUILD_DIR: &str = "/workspace/samples/cpp/build-rust-tests";
#[cfg(test)]
pub const PARSE_CPP: &str = "/workspace/samples/cpp/src/parse.cpp";
#[cfg(test)]
pub const PARSER_CPP: &str = "/workspace/samples/cpp/src/parser.cpp";
#[cfg(test)]
pub const PARSER_H: &str = "/workspace/samples/cpp/src/parser.h";
#[cfg(test)]
pub const NODE_H: &str = "/workspace/samples/cpp/include/yaml-cpp/node/node.h";
#[cfg(test)]
pub const CONVERT_H: &str = "/workspace/samples/cpp/include/yaml-cpp/node/convert.h";
#[cfg(test)]
pub const EMIT_FROM_EVENTS_H: &str = "/workspace/samples/cpp/include/yaml-cpp/emitfromevents.h";

#[cfg(test)]
pub fn ensure_test_build() {
    static INIT: OnceLock<Result<(), String>> = OnceLock::new();
    let result = INIT.get_or_init(|| {
        let compile_db = format!("{TEST_BUILD_DIR}/compile_commands.json");
        if Path::new(&compile_db).is_file() {
            return Ok(());
        }

        let status = Command::new("cmake")
            .args([
                "-S",
                "/workspace/samples/cpp",
                "-B",
                TEST_BUILD_DIR,
                "-G",
                "Ninja",
                "-D",
                "CMAKE_CXX_COMPILER=clang++",
                "-D",
                "CMAKE_EXPORT_COMPILE_COMMANDS=ON",
            ])
            .status()
            .map_err(|e| format!("failed to run cmake configure: {e}"))?;

        if !status.success() {
            return Err("cmake configure failed for test fixture".to_string());
        }

        if !Path::new(&compile_db).is_file() {
            return Err(format!("compile database not generated at {compile_db}"));
        }

        Ok(())
    });

    if let Err(msg) = result {
        panic!("{msg}");
    }
}
