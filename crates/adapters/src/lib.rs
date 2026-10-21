//! Adapters between AgentDeck and the agent CLIs.
//!
//! For now this crate holds the read-only history readers (`history`). The live adapters
//! (Claude stream-json, ACP, Codex app-server) arrive with M3 and M5.

pub mod agent;
pub mod history;

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
