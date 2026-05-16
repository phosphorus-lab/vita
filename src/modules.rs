//! Source loading and local module resolution.
//!
//! Vita's parser already understands `use` items. This loader turns those
//! imports into a single item list by recursively parsing local `.vita` files.
//! It intentionally keeps name resolution global for now, matching the current
//! semantic environment, while giving the compiler a real multi-file input path.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::syntax::ast::{Item, UseItem};
use crate::syntax::lexer::Lexer;
use crate::syntax::parser::Parser;

pub type LoadResult<T> = std::result::Result<T, String>;

/// Load a root Vita file and every local module it imports.
pub fn load_items(path: impl AsRef<Path>) -> LoadResult<Vec<Item>> {
    let root = path.as_ref();
    let root_dir = root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut loader = ModuleLoader::new(root_dir);
    loader.load_file(root)
}

struct ModuleLoader {
    root_dir: PathBuf,
    visited: HashSet<PathBuf>,
}

impl ModuleLoader {
    fn new(root_dir: PathBuf) -> Self {
        Self {
            root_dir,
            visited: HashSet::new(),
        }
    }

    fn load_file(&mut self, path: &Path) -> LoadResult<Vec<Item>> {
        let canonical = fs::canonicalize(path)
            .map_err(|err| format!("could not resolve '{}': {}", path.display(), err))?;

        if !self.visited.insert(canonical.clone()) {
            return Ok(Vec::new());
        }

        let source = fs::read_to_string(&canonical)
            .map_err(|err| format!("error reading '{}': {}", canonical.display(), err))?;
        let items = parse_source(&source)
            .map_err(|err| format!("error parsing '{}': {}", canonical.display(), err))?;

        let mut loaded = Vec::new();
        for item in &items {
            if let Item::Use(use_item) = item {
                let module_path = self.resolve_use(&canonical, use_item)?;
                loaded.extend(self.load_file(&module_path)?);
            }
        }

        loaded.extend(items);
        Ok(loaded)
    }

    fn resolve_use(&self, current_file: &Path, use_item: &UseItem) -> LoadResult<PathBuf> {
        let current_dir = current_file.parent().unwrap_or_else(|| Path::new("."));
        let mut segments = use_item.path.as_slice();
        let mut base = current_dir.to_path_buf();

        let leading_dots = segments
            .iter()
            .take_while(|segment| segment.as_str() == ".")
            .count();

        if leading_dots > 0 {
            for _ in 1..leading_dots {
                base.pop();
            }
            segments = &segments[leading_dots..];
        }

        let candidates = if leading_dots > 0 {
            candidate_paths(&base, segments)
        } else {
            let mut paths = candidate_paths(current_dir, segments);
            paths.extend(candidate_paths(&self.root_dir, segments));
            paths
        };

        for candidate in candidates {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }

        Err(format!(
            "could not resolve import '{}' from '{}'",
            use_item.path.join("."),
            current_file.display()
        ))
    }
}

fn parse_source(source: &str) -> LoadResult<Vec<Item>> {
    let tokens = Lexer::tokenize(source).map_err(|err| err.to_string())?;
    let mut parser = Parser::new(tokens);
    parser.parse().map_err(|err| err.to_string())
}

fn candidate_paths(base: &Path, segments: &[String]) -> Vec<PathBuf> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut joined = base.to_path_buf();
    for segment in segments {
        joined.push(segment);
    }

    let mut file = joined.clone();
    file.set_extension("vita");

    vec![file, joined.join("mod.vita")]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn loads_local_imports_before_root_items() {
        let dir = temp_module_dir("local_imports");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("main.vita"),
            r#"
                use helper

                fn main() {
                    print(message());
                }
            "#,
        )
        .unwrap();
        fs::write(
            dir.join("helper.vita"),
            r#"
                fn message() -> str {
                    "loaded"
                }
            "#,
        )
        .unwrap();

        let items = load_items(dir.join("main.vita")).unwrap();
        assert!(matches!(&items[0], Item::Fn(function) if function.name == "message"));
        assert!(items
            .iter()
            .any(|item| matches!(item, Item::Fn(function) if function.name == "main")));
    }

    #[test]
    fn loads_nested_mod_files_once() {
        let dir = temp_module_dir("nested_mods");
        fs::create_dir_all(dir.join("util")).unwrap();
        fs::write(
            dir.join("main.vita"),
            r#"
                use util
                use util

                fn main() {
                    print(answer());
                }
            "#,
        )
        .unwrap();
        fs::write(
            dir.join("util").join("mod.vita"),
            r#"
                fn answer() -> i32 {
                    42
                }
            "#,
        )
        .unwrap();

        let items = load_items(dir.join("main.vita")).unwrap();
        let answer_count = items
            .iter()
            .filter(|item| matches!(item, Item::Fn(function) if function.name == "answer"))
            .count();
        assert_eq!(answer_count, 1);
    }

    fn temp_module_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("vita_modules_{}_{}", label, unique))
    }
}
