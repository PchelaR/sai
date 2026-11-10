pub fn shell(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}
