pub fn shell(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::shell;

    #[test]
    fn wraps_in_single_quotes() {
        assert_eq!(shell("a b"), "'a b'");
    }

    #[test]
    fn escapes_single_quotes() {
        assert_eq!(shell("it's"), r"'it'\''s'");
    }
}
