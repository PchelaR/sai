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
