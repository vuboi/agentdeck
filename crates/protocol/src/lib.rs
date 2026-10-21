//! Stub library for the ag-protocol workspace crate.

/// Identifies this crate as part of the AgentDeck scaffold.
pub const PROTOCOL_STUB: &str = "ag-protocol";

#[cfg(test)]
mod tests {
    use super::PROTOCOL_STUB;

    #[test]
    fn stub_has_expected_name() {
        assert_eq!(PROTOCOL_STUB, "ag-protocol");
    }
}
