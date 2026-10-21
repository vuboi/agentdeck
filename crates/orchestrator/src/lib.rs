//! Stub library for the ag-orchestrator workspace crate.

/// Identifies this crate as part of the AgentDeck scaffold.
pub const ORCHESTRATOR_STUB: &str = "ag-orchestrator";

#[cfg(test)]
mod tests {
    use super::ORCHESTRATOR_STUB;

    #[test]
    fn stub_has_expected_name() {
        assert_eq!(ORCHESTRATOR_STUB, "ag-orchestrator");
    }
}
