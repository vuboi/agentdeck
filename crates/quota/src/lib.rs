//! Stub library for the ag-quota workspace crate.

/// Identifies this crate as part of the AgentDeck scaffold.
pub const QUOTA_STUB: &str = "ag-quota";

#[cfg(test)]
mod tests {
    use super::QUOTA_STUB;

    #[test]
    fn stub_has_expected_name() {
        assert_eq!(QUOTA_STUB, "ag-quota");
    }
}
