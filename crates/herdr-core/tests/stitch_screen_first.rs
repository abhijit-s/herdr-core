//! `terminal.read` with `screen_first`: the screen at once, the stitched history from memory later.
#![cfg(unix)] // Windows: tests/windows.rs

mod common;

use common::*;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn session() -> Session {
    let mut s = Session::new();
    s.env.push(("HC_STITCH_MS".into(), "8000".into()));
    let config = s.root.join("config/herdr-core/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(config, "headless = \"40x12\"\n").unwrap();
    s
}

fn app(s: &Session, flags: &[&str]) -> String {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake_tui.py");
    let mut args = vec!["run", "--bg", python3(), script];
    args.extend_from_slice(flags);
    let t = s.json(&args)["address"].as_str().unwrap().to_string();
    wait_until("app drawn", || s.json(&["read", &t])["text"].as_str().unwrap().contains("status"));
    t
}

fn screen(s: &Session, t: &str) -> String {
    s.json(&["read", t])["text"].as_str().unwrap().to_string()
}

fn wheels(s: &Session, t: &str) -> Option<String> {
    screen(s, t).split("wheels=").nth(1).and_then(|w| w.split_whitespace().next()).map(String::from)
}

fn whole(text: &str, n: usize) -> bool {
    let got: Vec<&str> = text.lines().filter(|l| l.starts_with("line ")).map(str::trim_end).collect();
    let want: Vec<String> = (1..=n).map(|i| format!("line {i:03}")).collect();
    got == want
}

fn screen_first(conn: &mut Conn, t: &str) -> Value {
    let r = conn.call(1, "terminal.read", json!({"terminal": t, "lines": 200, "screen_first": true}));
    r["result"].clone()
}

#[test]
fn the_screen_comes_at_once_and_the_history_from_memory_after() {
    let s = session();
    let t = app(&s, &["--lines", "50", "--slow", "30"]);
    let mut conn = s.connect();

    let started = Instant::now();
    let first = screen_first(&mut conn, &t);
    assert!(started.elapsed() < Duration::from_millis(500), "took {:?}", started.elapsed());
    assert_eq!((first["how"].as_str(), first["pending"].as_bool()), (Some("screen"), Some(true)), "{first}");
    assert_eq!(first["complete"], false);
    assert!(first["text"].as_str().unwrap().contains("line 050"), "{first}");

    // Asking again while it scrolls starts nothing new; once done, it answers from memory.
    let mut last = first;
    wait_until_for("stitched in the background", Duration::from_secs(20), || {
        last = screen_first(&mut conn, &t);
        last["pending"] != true
    });
    assert_eq!((last["how"].as_str(), last["complete"].as_bool()), (Some("stitched"), Some(true)), "{last}");
    assert!(whole(last["text"].as_str().unwrap(), 50), "{}", last["text"]);
    assert!(last.get("pending").is_none(), "pending is only sent when true: {last}");

    // The plain read is the same, from memory: no more scrolling.
    wait_until("view restored", || screen(&s, &t).contains("line 050"));
    let before = wheels(&s, &t);
    let plain = s.json(&["read", &t, "--lines", "200"]);
    assert_eq!(plain["text"], last["text"]);
    assert_eq!(wheels(&s, &t), before);
}

#[test]
fn many_screen_first_reads_scroll_only_once() {
    let s = session();
    let flags = ["--lines", "50", "--slow", "30"];
    let once = app(&s, &flags);
    assert_eq!(s.json(&["read", &once, "--lines", "200"])["complete"], true);
    wait_until("view restored", || screen(&s, &once).contains("line 050"));

    let t = app(&s, &flags);
    let mut conn = s.connect();
    for _ in 0..5 {
        assert_eq!(screen_first(&mut conn, &t)["pending"], true);
    }
    wait_until_for("stitched", Duration::from_secs(20), || screen_first(&mut conn, &t)["pending"] != true);
    wait_until("view restored", || screen(&s, &t).contains("line 050"));
    assert_eq!(wheels(&s, &t), wheels(&s, &once), "one stitch's worth of wheel input");
}

#[test]
fn the_cli_prints_the_screen_then_the_whole_read() {
    let s = session();
    let t = app(&s, &["--lines", "50"]);
    let out = s.hc(&["--json", "read", &t, "--lines", "200", "--screen-first"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let answers: Vec<Value> =
        String::from_utf8(out.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(answers.len(), 2, "{answers:?}");
    assert_eq!((answers[0]["how"].as_str(), answers[0]["pending"].as_bool()), (Some("screen"), Some(true)));
    assert_eq!((answers[1]["how"].as_str(), answers[1]["complete"].as_bool()), (Some("stitched"), Some(true)));
    assert!(whole(answers[1]["text"].as_str().unwrap(), 50), "{}", answers[1]["text"]);

    // From memory now: one answer, already whole.
    let human = String::from_utf8(s.hc(&["read", &t, "--lines", "200", "--screen-first"]).stdout).unwrap();
    assert!(!human.contains("reading its history"), "{human}");
    assert!(whole(&human, 50), "{human}");
}

#[test]
fn plain_and_busy_terminals_give_one_answer() {
    let s = session();
    let shell = s.json(&["run", "--bg", "sh", "-c", "seq 1 30; sleep 60"])["address"].as_str().unwrap().to_string();
    wait_until("printed", || screen(&s, &shell).contains("30"));
    let busy = app(&s, &["--lines", "50", "--blocked"]);
    for (t, how) in [(&shell, "history"), (&busy, "screen")] {
        let out = s.hc(&["--json", "read", t, "--lines", "200", "--screen-first"]);
        let text = String::from_utf8(out.stdout).unwrap();
        assert_eq!(text.lines().count(), 1, "{text}");
        let r: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(r["how"], how, "{r}");
        assert!(r.get("pending").is_none(), "{r}");
    }
    assert!(screen(&s, &busy).contains("wheels=0 "), "a busy app is never scrolled");
}
