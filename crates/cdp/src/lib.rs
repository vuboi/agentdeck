//! Stub library for the ag-cdp workspace crate.

/// Identifies this crate as part of the AgentDeck scaffold.
pub const CDP_STUB: &str = "ag-cdp";

#[cfg(test)]
mod tests {
    use super::CDP_STUB;

    #[test]
    fn stub_has_expected_name() {
        assert_eq!(CDP_STUB, "ag-cdp");
    }
}
