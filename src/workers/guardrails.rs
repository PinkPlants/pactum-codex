#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    fn worker_module_paths() -> Vec<std::path::PathBuf> {
        let workers_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/workers");
        let entries = fs::read_dir(&workers_dir).unwrap_or_else(|e| {
            panic!("failed to read workers dir {}: {e}", workers_dir.display())
        });

        let mut paths: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
            .filter(|path| path.file_name().is_none_or(|name| name != "guardrails.rs"))
            .collect();

        paths.sort();
        paths
    }

    #[test]
    fn worker_modules_must_not_call_process_exit() {
        for path in worker_module_paths() {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

            assert!(
                !source.contains("std::process::exit(") && !source.contains("process::exit("),
                "forbidden process-wide exit found in {}",
                path.display()
            );
        }
    }

    #[test]
    fn guardrail_scope_explicitly_excludes_startup_fail_fast_files() {
        let startup_files = [
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/config.rs"),
        ];

        let startup_source = startup_files
            .iter()
            .map(|path| {
                fs::read_to_string(path)
                    .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            startup_source.contains("FailFast") && startup_source.contains("FAIL-FAST"),
            "startup fail-fast policy markers missing; guardrail scope assumptions outdated"
        );
    }
}
