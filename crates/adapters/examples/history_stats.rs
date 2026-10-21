//! Prints counts for the real Claude/Codex history on this machine (never any message text).
//! Run: `cargo run -p ag-adapters --example history_stats`

use std::{path::PathBuf, time::Instant};

use ag_adapters::history::{list_all, load, Roots, Source};

fn main() {
    let home = PathBuf::from(
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .expect("home dir"),
    );
    let roots = Roots::from_home(&home);

    let started = Instant::now();
    let sessions = list_all(&roots);
    println!(
        "indexed {} sessions in {:?}",
        sessions.len(),
        started.elapsed()
    );
    for source in [Source::Claude, Source::Codex] {
        let of: Vec<_> = sessions.iter().filter(|s| s.source == source).collect();
        let forks = of.iter().filter(|s| s.parent_id.is_some()).count();
        let projects: std::collections::BTreeSet<_> = of.iter().map(|s| s.cwd.as_str()).collect();
        println!(
            "{source:?}: {} sessions, {forks} linked as forks, {} distinct folders",
            of.len(),
            projects.len()
        );
    }

    let (mut ok, mut empty, mut blocks, mut commands) = (0usize, 0usize, 0usize, 0usize);
    let started = Instant::now();
    for session in &sessions {
        match load(&roots, session.source, std::path::Path::new(&session.path)) {
            Ok(messages) if messages.is_empty() => empty += 1,
            Ok(messages) => {
                ok += 1;
                let users = messages.iter().filter(|m| m.role == "user").count();
                if users < 2 && messages.len() > 4 {
                    println!(
                        "{:?} {}: {users} user / {} messages",
                        session.source,
                        session.id,
                        messages.len()
                    );
                }
                for message in &messages {
                    blocks += message.blocks.len();
                    commands += message
                        .blocks
                        .iter()
                        .filter(|b| matches!(b, ag_adapters::history::HistoryBlock::Command { .. }))
                        .count();
                }
            }
            Err(error) => println!("load error: {error}"),
        }
    }
    println!(
        "loaded {ok} transcripts ({empty} empty) in {:?}: {blocks} blocks, {commands} tool calls",
        started.elapsed()
    );
}
