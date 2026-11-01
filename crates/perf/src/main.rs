//! Measures a built browser and checks it against budgets: `cargo run -p perf -- dist`.
//! Needs a display, so run it under `xvfb-run` where there isn't one. Linux only for
//! now, because it reads memory and CPU use from `/proc`.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio, exit};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const SETTLE: Duration = Duration::from_secs(10);
const CPU_SAMPLE: Duration = Duration::from_secs(10);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);

/// A limit for a measurement. These sit somewhat above typical results on a modest
/// laptop, to catch regressions rather than ordinary noise between machines.
struct Budget {
    name: &'static str,
    unit: &'static str,
    limit: u64,
}

const INSTALLED_SIZE: Budget = Budget {
    name: "installed size",
    unit: "MB",
    limit: 450,
};
const STARTUP: Budget = Budget {
    name: "startup to first request",
    unit: "ms",
    limit: 15_000,
};
const ONE_TAB_RAM: Budget = Budget {
    name: "RAM with one simple tab",
    unit: "MB",
    limit: 500,
};
const IDLE_CPU: Budget = Budget {
    name: "idle CPU, % of one core",
    unit: "%",
    limit: 5,
};

fn main() {
    let bundle = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dist"));
    let binary = bundle.join(if cfg!(windows) {
        "stardust.exe"
    } else {
        "stardust"
    });
    if !binary.is_file() {
        eprintln!(
            "No browser at {}. Run `cargo xtask bundle --release` first.",
            binary.display()
        );
        exit(2);
    }

    let results = measure(&bundle, &binary);
    let mut failed = false;
    for (budget, value) in &results {
        let status = if value > &budget.limit { "OVER" } else { "ok" };
        failed |= value > &budget.limit;
        println!(
            "{status:<5} {:<28} {value:>7} {} (limit {})",
            budget.name, budget.unit, budget.limit
        );
    }
    if failed {
        exit(1);
    }
}

fn measure(bundle: &Path, binary: &Path) -> Vec<(&'static Budget, u64)> {
    let size_mb = folder_size(bundle) / (1024 * 1024);
    let profile = std::env::temp_dir().join(format!("browser-perf-{}", std::process::id()));
    std::fs::create_dir_all(&profile).expect("Failed to create a temporary profile");

    let (url, first_request) = serve_test_page();
    let started = Instant::now();
    let mut browser = Command::new(binary)
        .arg(format!("--url={url}"))
        .env("XDG_DATA_HOME", &profile)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start the browser");

    let Ok(()) = first_request.recv_timeout(STARTUP_TIMEOUT) else {
        stop(&mut browser, binary, &profile);
        eprintln!("The browser never requested the test page.");
        exit(1);
    };
    let startup_ms = started.elapsed().as_millis() as u64;

    thread::sleep(SETTLE);
    let ram_mb = processes_of(binary)
        .iter()
        .map(|pid| pss_kb(*pid))
        .sum::<u64>()
        / 1024;
    let cpu_before = cpu_ticks(binary);
    thread::sleep(CPU_SAMPLE);
    let cpu_used = cpu_ticks(binary).saturating_sub(cpu_before);
    let idle_cpu = cpu_used * 100 / (CPU_SAMPLE.as_secs() * clock_ticks_per_second());

    stop(&mut browser, binary, &profile);
    vec![
        (&INSTALLED_SIZE, size_mb),
        (&STARTUP, startup_ms),
        (&ONE_TAB_RAM, ram_mb),
        (&IDLE_CPU, idle_cpu),
    ]
}

/// Serves one page on a local port, reporting when the first request arrives.
fn serve_test_page() -> (String, mpsc::Receiver<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to open a local port");
    let port = listener
        .local_addr()
        .expect("Failed to read the port")
        .port();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        const BODY: &str = "<!doctype html><title>perf</title><h1>Performance test</h1>";
        for mut stream in listener.incoming().flatten() {
            let mut request = [0; 2048];
            let _ = stream.read(&mut request);
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{BODY}",
                BODY.len()
            );
            let _ = sender.send(());
        }
    });
    (format!("http://127.0.0.1:{port}/"), receiver)
}

fn stop(browser: &mut Child, binary: &Path, profile: &Path) {
    let _ = browser.kill();
    let _ = browser.wait();
    for pid in processes_of(binary) {
        let _ = Command::new("kill")
            .arg(pid.to_string())
            .stderr(Stdio::null())
            .status();
    }
    thread::sleep(Duration::from_secs(1));
    let _ = std::fs::remove_dir_all(profile);
}

fn folder_size(folder: &Path) -> u64 {
    std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(metadata) if metadata.is_dir() => folder_size(&entry.path()),
            Ok(metadata) => metadata.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Every process whose executable is this browser, including its helpers.
fn processes_of(binary: &Path) -> Vec<u32> {
    let Ok(binary) = binary.canonicalize() else {
        return Vec::new();
    };
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            let executable = std::fs::read_link(entry.path().join("exe")).ok()?;
            (executable == binary).then_some(pid)
        })
        .collect()
}

/// Memory in use, with shared pages divided among the processes sharing them.
fn pss_kb(pid: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup"))
        .ok()
        .and_then(|rollup| parse_pss_kb(&rollup))
        .unwrap_or(0)
}

fn parse_pss_kb(rollup: &str) -> Option<u64> {
    rollup
        .lines()
        .find(|line| line.starts_with("Pss:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn cpu_ticks(binary: &Path) -> u64 {
    processes_of(binary)
        .iter()
        .filter_map(|pid| std::fs::read_to_string(format!("/proc/{pid}/stat")).ok())
        .filter_map(|stat| parse_cpu_ticks(&stat))
        .sum()
}

/// User plus system time. The process name can contain spaces, so count from after it.
fn parse_cpu_ticks(stat: &str) -> Option<u64> {
    let after_name = stat.rsplit_once(')')?.1;
    let mut fields = after_name.split_whitespace().skip(11);
    Some(fields.next()?.parse::<u64>().ok()? + fields.next()?.parse::<u64>().ok()?)
}

fn clock_ticks_per_second() -> u64 {
    Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_memory_from_proc() {
        let rollup = "00-ff ---p 0 00:00 0 [rollup]\nRss: 2000 kB\nPss:  1234 kB\nPss_Anon: 5 kB\n";
        assert_eq!(parse_pss_kb(rollup), Some(1234));
        assert_eq!(parse_pss_kb("nothing"), None);
    }

    #[test]
    fn reads_cpu_time_when_the_process_name_has_spaces() {
        let stat = "42 (my (odd) name) S 1 2 3 4 5 6 7 8 9 10 111 222 0 0";
        assert_eq!(parse_cpu_ticks(stat), Some(333));
        assert_eq!(parse_cpu_ticks("garbage"), None);
    }
}
