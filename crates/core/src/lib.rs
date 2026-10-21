//! Core daemon library for AgentDeck.

/// Returns the daemon's product name.
pub const DAEMON_NAME: &str = "agentd";

#[cfg(test)]
mod tests {
    use super::DAEMON_NAME;

    #[test]
    fn daemon_name_is_stable() {
        assert_eq!(DAEMON_NAME, "agentd");
    }
}
