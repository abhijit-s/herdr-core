#![cfg(unix)] // Windows: tests/windows.rs

mod common;

use common::*;
use serde_json::Value;
use std::time::{Duration, Instant};

fn session() -> Session {
    let mut s = Session::new();
    s.env.push(("HC_QUIET_MS".into(), "0".into()));
    s.env.push(("HC_STARTED_MS".into(), "1500".into()));
    s
}

/// A fake agent: `r <report>` sends OSC 7501, `w <name>` waits for a file.
fn agent(s: &Session, body: &str) -> String {
    s.json(&["run", "--bg", "sh", "-c", &script(s, body)])["address"].as_str().unwrap().to_string()
}

fn state(s: &Session, t: &str) -> Value {
    s.json(&["get", t])["status"]["state"].clone()
}

/// Starts `hc wait <args>` in the background; join to get its JSON reply.
fn wait_bg(s: &Session, args: &[&str]) -> std::thread::JoinHandle<Value> {
    let mut cmd = s.cmd();
    cmd.arg("wait").args(args).args(["--timeout", "10"]);
    let handle = std::thread::spawn(move || {
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        serde_json::from_slice(&out.stdout).unwrap()
    });
    std::thread::sleep(Duration::from_millis(300)); // let it subscribe
    handle
}

#[test]
fn wait_ends_when_work_stops_and_says_the_exact_word() {
    let s = session();
    let t = agent(&s, "r state=working; w a; r state=done:msg=b2s=; sleep 100");
    wait_until("working", || state(&s, &t) == "working");
    let waiting = wait_bg(&s, &[&t]);
    touch(&s, "a");
    let r = waiting.join().unwrap();
    assert_eq!((r["ended"].as_bool(), r["reason"].as_str()), (Some(true), Some("status")));
    assert_eq!((r["status"].as_str(), r["turn"].as_u64()), (Some("done"), Some(1)));
}

#[test]
fn a_wait_on_what_is_not_working_returns_at_once_and_a_timeout_is_a_check_in() {
    let s = session();
    let t = agent(&s, "r state=done; w go; r state=working; sleep 100");
    wait_until("done", || state(&s, &t) == "done");
    let started = Instant::now();
    let r = s.json(&["wait", &t, "--timeout", "5"]);
    assert!(started.elapsed() < Duration::from_secs(2), "it already stopped working");
    assert_eq!(
        (r["ended"].as_bool(), r["reason"].as_str(), r["status"].as_str()),
        (Some(true), Some("status"), Some("done"))
    );

    touch(&s, "go");
    wait_until("working", || state(&s, &t) == "working");
    let r = s.json(&["wait", &t, "--timeout", "0.5"]);
    assert_eq!(
        (r["ended"].as_bool(), r["reason"].as_str(), r["status"].as_str()),
        (Some(false), Some("timeout"), Some("working"))
    );
}

#[test]
fn an_error_ends_an_output_wait_too() {
    let s = session();
    let t = agent(&s, "r state=working; w a; r state=error; sleep 100");
    wait_until("working", || state(&s, &t) == "working");
    let waiting = wait_bg(&s, &[&t, "--output", "never printed"]);
    touch(&s, "a");
    let r = waiting.join().unwrap();
    assert_eq!(
        (r["ended"].as_bool(), r["reason"].as_str(), r["status"].as_str()),
        (Some(true), Some("status"), Some("error"))
    );
}

#[test]
fn a_quick_flicker_is_not_missed() {
    let s = session();
    let t = agent(&s, "r state=working; w a; r state=blocked:kind=permission; r state=working; sleep 100");
    wait_until("working", || state(&s, &t) == "working");
    let waiting = wait_bg(&s, &[&t]);
    touch(&s, "a");
    assert_eq!(waiting.join().unwrap()["status"], "blocked");
}

#[test]
fn a_program_that_never_reports_status_is_working_until_it_exits() {
    let s = session();
    let t = agent(&s, "w a; exit 4");
    let waiting = wait_bg(&s, &[&t]);
    std::thread::sleep(Duration::from_millis(500));
    assert!(!waiting.is_finished(), "no status is not 'not working'");
    touch(&s, "a");
    let r = waiting.join().unwrap();
    assert_eq!((r["reason"].as_str(), r["exit_code"].as_i64()), (Some("exit"), Some(4)));
}

#[test]
fn exits_and_output_end_waits() {
    let s = session();
    let t = agent(&s, "r state=working; echo READY-early; w a; r state=idle; w a2; echo READY-now; w b; exit 4");
    wait_until("early output", || s.json(&["read", &t])["text"].as_str().unwrap().contains("READY-early"));
    let waiting = wait_bg(&s, &[&t, "--output", "READY"]);
    touch(&s, "a");
    wait_until("idle", || state(&s, &t) == "idle");
    std::thread::sleep(Duration::from_millis(200));
    touch(&s, "a2");
    let r = waiting.join().unwrap();
    assert_eq!(r["reason"], "output", "not output from before the wait, and not going idle");

    let waiting = wait_bg(&s, &[&t, "--output", "never printed"]);
    touch(&s, "b");
    let r = waiting.join().unwrap();
    assert_eq!((r["reason"].as_str(), r["exit_code"].as_i64()), (Some("exit"), Some(4)));
    // Nothing can change any more: a wait on a finished terminal returns at once.
    assert_eq!(s.json(&["wait", &t])["reason"], "exit");
}

#[test]
fn a_wait_after_send_catches_the_turn_even_if_it_already_ended() {
    let s = session();
    let t = agent(&s, "stty -echo; r state=idle; while read -r l; do r state=working; r state=done; done");
    wait_until("idle", || state(&s, &t) == "idle");
    s.json(&["send", &t, "go"]);
    wait_until("turn over", || s.json(&["get", &t])["turn"] == 1);
    let started = Instant::now();
    let r = s.json(&["wait", &t, "--timeout", "5"]);
    assert_eq!((r["ended"].as_bool(), r["status"].as_str(), r["turn"].as_u64()), (Some(true), Some("done"), Some(1)));
    assert!(started.elapsed() < Duration::from_secs(2));
    // Still not working: asking again gets the same answer at once.
    assert_eq!(s.json(&["wait", &t, "--timeout", "5"])["status"], "done");

    let r = s.json(&["send", "--wait", &t, "again"]);
    assert_eq!(
        (r["started"].as_bool(), r["wait"]["status"].as_str(), r["wait"]["turn"].as_u64()),
        (Some(true), Some("done"), Some(2))
    );
}

#[test]
fn a_retry_after_a_timeout_still_waits_for_the_sent_turn() {
    let s = session();
    let t = agent(
        &s,
        "stty -echo; r state=idle; read -r l; r state=working; w a; r state=done; r state=working; sleep 100",
    );
    wait_until("idle", || state(&s, &t) == "idle");
    s.json(&["send", &t, "task"]);
    let r = s.json(&["wait", &t, "--timeout", "0.05"]);
    assert_eq!(r["reason"], "timeout");
    touch(&s, "a"); // the sent turn ends, and new work starts
    wait_until("next turn", || s.json(&["get", &t])["turn"] == 1 && state(&s, &t) == "working");
    let r = s.json(&["wait", &t, "--timeout", "2"]);
    assert_eq!((r["ended"].as_bool(), r["turn"].as_u64()), (Some(true), Some(1)), "{r}");
}

#[test]
fn a_send_that_did_not_start_work_is_never_reported_finished() {
    let s = session();
    let t = agent(&s, "stty -echo; r state=idle; read -r l; w go; r state=working; r state=done; sleep 100");
    wait_until("idle", || state(&s, &t) == "idle");
    let r = s.json(&["send", "--wait", &t, "task", "--timeout", "10"]);
    assert_eq!(r["started"], false);
    let w = &r["wait"];
    assert_eq!(
        (w["ended"].as_bool(), w["reason"].as_str(), w["status"].as_str()),
        (Some(false), Some("not_started"), Some("idle"))
    );
    // Only slow: a plain wait still follows that send's turn instead of trusting "idle".
    let waiting = wait_bg(&s, &[&t]);
    touch(&s, "go");
    let r = waiting.join().unwrap();
    assert_eq!((r["ended"].as_bool(), r["status"].as_str(), r["turn"].as_u64()), (Some(true), Some("done"), Some(1)));
}

/// A shell with job control running `jobs` (fake agents, each its own file)
/// in front: the first at once, each later one after `w go<n>`. Then it waits
/// at its prompt.
fn shell_running(s: &Session, jobs: &[&str]) -> String {
    let mut line = String::from("set -m;");
    for (i, body) in jobs.iter().enumerate() {
        let path = s.root.join(format!("job{i}"));
        std::fs::write(&path, script(s, body)).unwrap();
        if i > 0 {
            line.push_str(&format!(" w go{i};"));
        }
        line.push_str(&format!(" sh {};", path.display()));
    }
    line.push_str(" read -r x");
    run(s, &["sh", "-c", &script(s, &line)])
}

fn polled() -> Session {
    let mut s = session();
    s.env.push(("HC_FOREGROUND_MS".into(), "30".into()));
    s
}

fn job_in_front(s: &Session, t: &str) -> bool {
    let r = get(s, t);
    r["foreground"]["pid"].as_i64().is_some_and(|p| Some(p) != r["pid"].as_i64())
}

/// Waits until the job in front said `word` and polls have seen it there.
fn job_said(s: &Session, t: &str, word: &str) {
    wait_until(word, || state(s, t) == word && job_in_front(s, t));
    std::thread::sleep(Duration::from_millis(300)); // many polls
}

#[test]
fn a_job_that_exits_to_its_shell_takes_its_status_with_it() {
    let s = polled();
    let t = shell_running(&s, &["r state=blocked:kind=permission; w a", "r state=working; w b"]);
    job_said(&s, &t, "blocked");
    let mut conn = s.connect();
    conn.call(1, "events.subscribe", serde_json::json!({"terminal": t}));
    touch(&s, "a");
    let ev = next_event(&mut conn, |m| m["params"]["type"] == "status");
    assert_eq!(ev["params"]["data"]["status"], Value::Null, "{ev}");
    assert_eq!(ev["params"]["data"]["reported"], true, "{ev}");

    touch(&s, "go1");
    job_said(&s, &t, "working");
    touch(&s, "b");
    let ev = next_event(&mut conn, |m| m["params"]["type"] == "turn");
    assert_eq!((ev["params"]["data"]["turn"].as_u64(), &ev["params"]["data"]["state"]), (Some(1), &Value::Null));
    let r = get(&s, &t);
    assert_eq!((&r["state"], &r["status"]), (&Value::from("live"), &Value::Null), "{r}");
}

#[test]
fn a_job_keeps_its_status_while_it_runs_its_own_tools_or_waits() {
    let s = polled();
    // A tool in the job's group, then (once seen) one in front as its own job.
    let t = shell_running(&s, &["r state=blocked; sleep 0.3; w a; set -m; sleep 0.3; set +m; r state=idle; w b"]);
    wait_until("the job blocked in front", || state(&s, &t) == "blocked" && job_in_front(&s, &t));
    std::thread::sleep(Duration::from_millis(400)); // the tool came and went
    assert_eq!(state(&s, &t), "blocked");
    touch(&s, "a");
    wait_until("its own job in front", || get(&s, &t)["foreground"]["argv"] == serde_json::json!(["sleep", "0.3"]));
    assert_eq!(state(&s, &t), "blocked");
    wait_until("idle", || state(&s, &t) == "idle");
    std::thread::sleep(Duration::from_millis(300)); // many polls
    assert_eq!(state(&s, &t), "idle", "alive and idle");
}

#[test]
fn the_programs_own_status_outlasts_its_jobs_and_ends_with_it() {
    let s = polled();
    let t = agent(&s, "r state=blocked; w a; set -m; sleep 0.3; w b; exit 0");
    wait_until("blocked", || state(&s, &t) == "blocked");
    std::thread::sleep(Duration::from_millis(300)); // seen in front
    touch(&s, "a");
    wait_until("a job in front", || get(&s, &t)["foreground"]["name"] == "sleep");
    wait_until("the program in front again", || !job_in_front(&s, &t));
    std::thread::sleep(Duration::from_millis(300)); // many polls
    assert_eq!(state(&s, &t), "blocked", "the job that ended didn't say it");
    touch(&s, "b");
    wait_until("finished", || get(&s, &t)["state"] == "finished");
    assert_eq!(get(&s, &t)["status"], Value::Null);
}

#[test]
fn a_jobs_status_survives_a_core_restart_or_crash_and_still_ends_with_it() {
    for (i, recovery) in ["restart", "crash"].into_iter().enumerate() {
        let s = polled();
        let gate = format!("a{i}");
        let t = shell_running(&s, &[&format!("r state=blocked; w {gate}")]);
        job_said(&s, &t, "blocked");
        let recover = || {
            if recovery == "restart" {
                s.json(&["server", "restart"]);
            } else {
                let pid = s.json(&["server", "status"])["pid"].as_i64().unwrap() as i32;
                assert_eq!(unsafe { libc::kill(pid, libc::SIGKILL) }, 0);
                wait_until("core absent", || !s.connect_ok());
            }
            assert_eq!(get(&s, &t)["state"], "live", "adopted");
            std::thread::sleep(Duration::from_millis(300)); // many polls
        };
        recover();
        assert_eq!(state(&s, &t), "blocked", "{recovery}");
        touch(&s, &gate);
        wait_until("reset", || get(&s, &t)["status"].is_null());
        recover(); // a crash replays the job's old reports from the holder
        assert_eq!(get(&s, &t)["status"], Value::Null, "{recovery}: it stays reset");
    }
}

#[test]
fn a_crash_does_not_blame_a_new_job_for_an_old_ones_exit() {
    let s = polled();
    let t = shell_running(&s, &["r state=blocked; w a", "r state=blocked; w b"]);
    job_said(&s, &t, "blocked");
    std::thread::sleep(Duration::from_millis(2500)); // its name saved (within SAVE_SOON)
    let pid = s.json(&["server", "status"])["pid"].as_i64().unwrap() as i32;
    assert_eq!(unsafe { libc::kill(pid, libc::SIGKILL) }, 0);
    wait_until("core absent", || !s.connect_ok());
    // While the core is down, the first job ends and the next says the same word.
    touch(&s, "a");
    touch(&s, "go1");
    std::thread::sleep(Duration::from_millis(500));
    wait_until("adopted, the next job in front", || job_in_front(&s, &t));
    std::thread::sleep(Duration::from_millis(300)); // many polls
    assert_eq!(state(&s, &t), "blocked", "the next job's word");
    touch(&s, "b");
    wait_until("reset when it ends", || get(&s, &t)["status"].is_null());
}
