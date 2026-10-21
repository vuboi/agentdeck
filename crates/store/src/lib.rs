//! Stub library for the ag-store workspace crate.

/// Identifies this crate as part of the AgentDeck scaffold.
pub const STORE_STUB: &str = "ag-store";

#[cfg(test)]
mod tests {
    use super::STORE_STUB;

    #[test]
    fn stub_has_expected_name() {
        assert_eq!(STORE_STUB, "ag-store");
    }
}
