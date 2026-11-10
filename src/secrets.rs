pub const PATTERNS: &[&str] = &[
    ".env",
    ".env.*",
    "*.pem",
    "*.key",
    "*.p12",
    "*.pfx",
    "*.jks",
    "*.keystore",
    "*.ppk",
    "*.gpg",
    "*.secret",
    "id_rsa*",
    "id_dsa*",
    "id_ecdsa*",
    "id_ed25519*",
    ".ssh/",
    ".gnupg/",
    ".aws/",
    ".azure/",
    ".gcloud/",
    ".kube/",
    ".docker/",
    "secrets/",
    ".secrets/",
    ".netrc",
    ".npmrc",
    ".pypirc",
    ".git-credentials",
    "credentials*.json",
    "service-account*.json",
    "*.tfvars",
    "*.tfstate",
    "*.tfstate.*",
];

pub fn names() -> Vec<&'static str> {
    PATTERNS
        .iter()
        .map(|pattern| pattern.trim_end_matches('/'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{PATTERNS, names};

    #[test]
    fn names_drop_trailing_slashes() {
        let names = names();
        assert_eq!(names.len(), PATTERNS.len());
        assert!(names.contains(&".ssh"));
        assert!(names.iter().all(|name| !name.ends_with('/')));
    }

    #[test]
    fn covers_common_secrets() {
        for pattern in [".env", ".env.*", "*.pem", "*.key", "id_ed25519*", ".aws/"] {
            assert!(PATTERNS.contains(&pattern), "{pattern} must be ignored");
        }
    }
}
