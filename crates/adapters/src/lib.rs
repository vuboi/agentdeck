//! Stub library for the ag-adapters workspace crate.

/// Identifies this crate as part of the AgentDeck scaffold.
pub const ADAPTERS_STUB: &str = "ag-adapters";

#[cfg(test)]
mod tests {
    use super::ADAPTERS_STUB;

    #[test]
    fn stub_has_expected_name() {
        assert_eq!(ADAPTERS_STUB, "ag-adapters");
    }
}
