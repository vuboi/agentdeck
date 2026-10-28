//! Keeps oversized tool output from filling a model's context window.
//!
//! The idea is DeepSeek Harness's tool-result pruner (https://github.com/deepseek-ai/deepseek-harness): a result over the
//! budget keeps its head and its tail around a short "middle pruned" marker, never its head alone. Slicing is by code
//! point with fixed budgets, so the result is never longer than the limit, always shorter than what it replaces, and cutting
//! it again changes nothing.

/// `text` when it fits in `limit` characters, else its beginning and its end around a marker that says how much was cut.
///
/// `tail_percent` is the share of the kept text taken from the end: a command's output ends with its verdict, so it keeps
/// mostly the end; a file or a page is read from the top, so it keeps mostly the start.
pub fn trim_middle(text: &str, limit: usize, tail_percent: usize) -> String {
    let count = text.chars().count();
    if count <= limit {
        return text.to_owned();
    }
    // The marker names how many characters are cut, which depends on how many are kept: assume the larger number of digits
    // first, then settle on the real one. It never needs more than two rounds.
    let mut marker = marker_for(count);
    let mut kept = limit.saturating_sub(marker.chars().count());
    if kept == 0 {
        // A limit too small for a marker: the end alone, which is what matters most.
        return text.chars().skip(count - limit).collect();
    }
    marker = marker_for(count - kept);
    kept = limit.saturating_sub(marker.chars().count()).min(count);
    let tail = kept * tail_percent.min(100) / 100;
    let head = kept - tail;
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(count - tail).collect();
    format!("{start}{marker}{end}")
}

fn marker_for(count: usize) -> String {
    format!("\n[… {count} characters pruned …]\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_within_the_limit_is_returned_as_it_is() {
        assert_eq!(trim_middle("short", 100, 50), "short");
        let exact = "a".repeat(40);
        assert_eq!(trim_middle(&exact, 40, 50), exact);
    }

    #[test]
    fn a_long_text_keeps_both_ends_and_fits_the_limit() {
        let text = format!("START{}END", "x".repeat(5000));
        let out = trim_middle(&text, 300, 50);
        assert!(out.chars().count() <= 300, "{} characters", out.chars().count());
        assert!(out.starts_with("START"));
        assert!(out.ends_with("END"));
        assert!(out.contains("characters pruned"));
        assert!(out.chars().count() < text.chars().count());
    }

    #[test]
    fn the_tail_share_decides_which_end_gets_more() {
        let text = format!("{}{}", "h".repeat(2000), "t".repeat(2000));
        let tail_heavy = trim_middle(&text, 400, 80);
        let head_heavy = trim_middle(&text, 400, 20);
        let count = |text: &str, c: char| text.chars().filter(|x| *x == c).count();
        assert!(count(&tail_heavy, 't') > count(&tail_heavy, 'h'));
        assert!(count(&head_heavy, 'h') > count(&head_heavy, 't'));
    }

    #[test]
    fn cutting_again_changes_nothing() {
        let text = "line of output\n".repeat(4000);
        let once = trim_middle(&text, 2000, 70);
        assert_eq!(trim_middle(&once, 2000, 70), once);
    }

    #[test]
    fn characters_are_never_split() {
        let text = "héllo 🙂 wörld ".repeat(500);
        let out = trim_middle(&text, 257, 50);
        // Slicing by code point keeps every kept character whole: this is valid UTF-8 by construction, so a wrong
        // boundary would have panicked above.
        assert!(out.chars().count() <= 257);
        assert!(out.contains("pruned"));
    }

    #[test]
    fn a_limit_too_small_for_the_marker_keeps_the_end() {
        let out = trim_middle("0123456789", 4, 50);
        assert_eq!(out, "6789");
    }
}
