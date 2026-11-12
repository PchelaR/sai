pub const BUILD: &[&str] = &[
    ".venv/",
    "venv/",
    "__pycache__/",
    ".tox/",
    ".nox/",
    ".mypy_cache/",
    ".pytest_cache/",
    ".ruff_cache/",
    ".hypothesis/",
    ".ipynb_checkpoints/",
    "*.egg-info/",
    "htmlcov/",
    "node_modules/",
    "bower_components/",
    ".next/",
    ".nuxt/",
    ".svelte-kit/",
    ".angular/",
    ".turbo/",
    ".parcel-cache/",
    ".expo/",
    ".nyc_output/",
    "target/",
    ".gradle/",
    ".kotlin/",
    ".dart_tool/",
    "DerivedData/",
    "_build/",
    ".stack-work/",
    "dist-newstyle/",
    ".zig-cache/",
    "zig-cache/",
    "zig-out/",
    "cmake-build-*/",
    ".terraform/",
    ".cache/",
    ".DS_Store",
    "Thumbs.db",
];

pub fn matches(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        Some((prefix, suffix)) => {
            name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
        }
        None => pattern == name,
    }
}

#[cfg(test)]
mod tests {
    use super::{BUILD, matches};

    #[test]
    fn matches_simple_globs() {
        assert!(matches("*.egg-info", "sai.egg-info"));
        assert!(matches("cmake-build-*", "cmake-build-debug"));
        assert!(!matches("*.egg-info", "egg-info"));
        assert!(matches("venv", "venv"));
        assert!(!matches("venv", "venvs"));
    }

    #[test]
    fn patterns_are_unique_and_have_at_most_one_star() {
        for (index, pattern) in BUILD.iter().enumerate() {
            assert!(
                !BUILD[..index].contains(pattern),
                "{pattern} is listed twice"
            );
            assert!(pattern.matches('*').count() <= 1, "{pattern}");
        }
    }
}
