const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

pub fn of(text: &str) -> String {
    let hash = text.bytes().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    });
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::of;

    #[test]
    fn matches_fnv1a_reference_values() {
        assert_eq!(of(""), "cbf29ce484222325");
        assert_eq!(of("a"), "af63dc4c8601ec8c");
        assert_eq!(of("foobar"), "85944171f73967e8");
    }
}
