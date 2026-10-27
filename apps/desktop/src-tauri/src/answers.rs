//! Answers for a start script that asks questions. A dev server AgentDeck starts has no person at its keyboard, so a
//! script that asks "1) project a  2) project b", then "port (default 4200)", waited for ever and the start failed. The
//! question is read from the script's output and answered here, from (in this order):
//!
//! - the answers the caller gave, in the order the questions come (the agent reads them from what the user asked);
//! - a port question: a free port, the one the caller expects if it is free;
//! - a numbered menu: the option whose words match the answer given, else the first;
//! - yes / no: the default, or yes for "continue"-like questions;
//! - anything else with a default: the default (Enter).
//!
//! Free of the app, so plain tests can run it.

use std::collections::VecDeque;
use std::net::TcpListener;

use regex::Regex;

/// Whether nothing listens on `port` (this machine's loopback).
pub fn port_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// The first free port from `from` on (at most a hundred are tried; `from` itself when none is).
pub fn free_port(from: u16) -> u16 {
    (from..from.saturating_add(100))
        .find(|port| port_free(*port))
        .unwrap_or(from)
}

/// `command` with the port it names (`--port 4200`, `--port=4200`, `-p 4200`) changed to `to`.
pub fn with_port(command: &str, from: u16, to: u16) -> String {
    let pattern = format!(r"(--port[= ]|-p[= ]?){from}\b");
    match Regex::new(&pattern) {
        Ok(found) => found.replace_all(command, format!("${{1}}{to}")).into_owned(),
        Err(_) => command.to_string(),
    }
}

/// What a script printed that makes it look like it is waiting: the last non-empty lines, trimmed.
fn lines_of(text: &str) -> Vec<&str> {
    text.lines().map(str::trim).filter(|line| !line.is_empty()).collect()
}

/// The numbered options of a menu in `lines`: `1) project a`, `2. project b`, `[3] c`, `> 4 - d`.
fn menu(lines: &[&str]) -> Vec<(String, String)> {
    let Ok(item) = Regex::new(r"^(?:[>❯›*]\s*)?[\[(]?(\d{1,2})[\])]?[).:\-]?\s+(\S.*)$") else {
        return Vec::new();
    };
    lines
        .iter()
        .filter_map(|line| item.captures(line))
        .map(|found| (found[1].to_string(), found[2].to_string()))
        .collect()
}

/// The words of `text` that say something (letters and digits, two or more of them), lower case.
fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() >= 2)
        .map(str::to_owned)
        .collect()
}

/// The number of the option that best matches what the caller wants (`want`), the first when none does.
fn choose(options: &[(String, String)], want: &str) -> String {
    let wanted = words(want);
    let best = options
        .iter()
        .map(|(number, label)| {
            let label = label.to_lowercase();
            let score = wanted.iter().filter(|word| label.contains(word.as_str())).count();
            (score, number)
        })
        .max_by_key(|(score, _)| *score);
    match best {
        Some((score, number)) if score > 0 => number.clone(),
        _ => options.first().map(|(number, _)| number.clone()).unwrap_or_default(),
    }
}

/// What to type for the question `text` ends with. `queue` holds the answers the caller gave, `expected` the port the
/// caller wants the server on. An empty answer is just Enter: the script's own default.
pub fn decide(text: &str, queue: &mut VecDeque<String>, expected: Option<u16>) -> String {
    let lines = lines_of(text);
    let Some(last) = lines.last() else { return String::new() };
    let question = last.to_lowercase();
    let options = menu(&lines);

    // A port question gets a port nobody uses.
    if question.contains("port") && options.len() < 2 {
        if let Some(given) = queue.front().filter(|given| given.trim().parse::<u16>().is_ok()) {
            let given = given.trim().to_string();
            queue.pop_front();
            return given;
        }
        return free_port(expected.unwrap_or(3000)).to_string();
    }

    // The caller's answers come in order; one that is not a number picks a menu option by its words.
    if let Some(given) = queue.pop_front() {
        if options.len() >= 2 && given.trim().parse::<u32>().is_err() {
            return choose(&options, &given);
        }
        return given;
    }

    if options.len() >= 2 {
        return choose(&options, "");
    }

    let yes_no = ["(y/n)", "[y/n]", "(yes/no)", "[yes/no]", "(y/n/"];
    if yes_no.iter().any(|mark| question.contains(mark)) {
        let default_yes = last.contains("[Y/n]") || last.contains("(Y/n)") || last.contains("[Yes/no]");
        let default_no = last.contains("[y/N]") || last.contains("(y/N)") || last.contains("[yes/No]");
        let goes_on = ["continue", "proceed", "start", "run", "open", "use ", "install", "launch"]
            .iter()
            .any(|word| question.contains(word));
        return if default_yes || default_no && !goes_on { String::new() } else { "y".to_string() };
    }

    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(text: &str, answers: &[&str]) -> String {
        let mut queue: VecDeque<String> = answers.iter().map(|answer| (*answer).to_owned()).collect();
        decide(text, &mut queue, Some(54_321))
    }

    #[test]
    fn a_menu_takes_the_option_the_user_named() {
        let text = "Which app?\n1) web admin\n2) web shop\nEnter a number: ";
        assert_eq!(asked(text, &["shop"]), "2");
        assert_eq!(asked(text, &["1"]), "1");
        assert_eq!(asked(text, &[]), "1", "no answer given: the first option");
    }

    #[test]
    fn a_port_question_gets_a_free_port_or_the_one_given() {
        let text = "Port (default 4200): ";
        let free = asked(text, &[]).parse::<u16>().expect("a number");
        assert!(port_free(free) || free == 54_321);
        assert_eq!(asked(text, &["5005"]), "5005");
    }

    #[test]
    fn a_taken_port_moves_on() {
        let taken = TcpListener::bind(("127.0.0.1", 0)).expect("a port");
        let port = taken.local_addr().expect("address").port();
        assert!(!port_free(port));
        assert_ne!(free_port(port), port);
    }

    #[test]
    fn yes_no_and_defaults_are_accepted() {
        assert_eq!(asked("Install the packages? (y/n) ", &[]), "y");
        assert_eq!(asked("Continue? [y/N] ", &[]), "y");
        assert_eq!(asked("Delete the cache? [y/N] ", &[]), "");
        assert_eq!(asked("Use the defaults? [Y/n] ", &[]), "");
        assert_eq!(asked("Name (my-app): ", &[]), "");
    }

    #[test]
    fn the_port_in_a_command_is_rewritten() {
        assert_eq!(with_port("vite --port 4200", 4200, 4201), "vite --port 4201");
        assert_eq!(with_port("vite --port=4200 --host", 4200, 4201), "vite --port=4201 --host");
        assert_eq!(with_port("serve -p 4200", 4200, 4201), "serve -p 4201");
        assert_eq!(with_port("vite --port 42000", 4200, 4201), "vite --port 42000");
        assert_eq!(with_port("pnpm start", 4200, 4201), "pnpm start");
    }
}
