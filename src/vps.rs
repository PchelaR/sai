pub const USER: &str = "sai";
pub const SSH_PORT: u16 = 28522;
pub const HOST_ALIAS: &str = "sai";
pub const MINIMAL: &str = "minimal";
pub const SETUP_MARKER: &str = "/etc/sai/setup-complete";
/// Present when the user chose to keep Docker's build cache: image rebuilds after sai
/// updates take minutes instead of half an hour, for a few GB of disk
pub fn build_cache_marker() -> String {
    format!("/home/{USER}/.sai/keep-build-cache")
}
