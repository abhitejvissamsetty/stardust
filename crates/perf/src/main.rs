//! Measures a built browser and checks it against budgets:
//! `cargo run --release -p perf -- dist [--chrome <path to Chrome>]`.
//!
//! Needs a display, so run it under `xvfb-run` where there isn't one. Works on Linux,
//! where memory is PSS from `/proc`, and macOS, where it's each process's footprint
//! as Activity Monitor shows it. The two aren't comparable with each other.
//!
//! With `--chrome`, the same 20 tabs open in Chrome with a fresh profile, and the
//! 20-tab budget becomes Chrome's own figure.

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
const TAB_COUNT: usize = 20;
/// Long enough for every page to load and for the browser's first check for idle
/// tabs, which runs a minute after startup.
const TWENTY_TAB_SETTLE: Duration = Duration::from_secs(75);

/// A limit for a measurement. These sit somewhat above typical results on a modest
/// laptop, to catch regressions rather than ordinary noise between machines.
struct Measurement {
    name: &'static str,
    unit: &'static str,
    value: u64,
    limit: u64,
}

const INSTALLED_SIZE_MB: u64 = 450;
const STARTUP_MS: u64 = 15_000;
const ONE_TAB_RAM_MB: u64 = 500;
const IDLE_CPU_PERCENT: u64 = 5;
/// Used when there's no Chrome to compare with.
const TWENTY_TABS_RAM_MB: u64 = 2_500;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut bundle = PathBuf::from("dist");
    let mut chrome = None;
    while let Some(arg) = args.next() {
        if arg == "--chrome" {
            chrome = args.next().map(PathBuf::from);
        } else {
            bundle = PathBuf::from(arg);
        }
    }
    let binary = browser_binary(&bundle);
    if !binary.is_file() {
        eprintln!(
            "No browser at {}. Run `cargo xtask bundle --release` first.",
            binary.display()
        );
        exit(2);
    }

    let mut results = measure_one_tab(&bundle, &binary);
    let stardust_tabs = measure_twenty_tabs(&binary, false);
    let chrome_tabs = chrome
        .as_deref()
        .map(|chrome| measure_twenty_tabs(chrome, true));
    results.push(Measurement {
        name: if chrome_tabs.is_some() {
            "RAM with 20 tabs, against Chrome"
        } else {
            "RAM with 20 tabs"
        },
        unit: "MB",
        value: stardust_tabs,
        limit: chrome_tabs.unwrap_or(TWENTY_TABS_RAM_MB),
    });

    let mut failed = false;
    for result in &results {
        let over = result.value > result.limit;
        failed |= over;
        println!(
            "{:<5} {:<34} {:>7} {} (limit {})",
            if over { "OVER" } else { "ok" },
            result.name,
            result.value,
            result.unit,
            result.limit
        );
    }
    if failed {
        exit(1);
    }
}

fn browser_binary(bundle: &Path) -> PathBuf {
    if cfg!(windows) {
        bundle.join("stardust.exe")
    } else if cfg!(target_os = "macos") {
        bundle.join("stardust.app/Contents/MacOS/stardust")
    } else {
        bundle.join("stardust")
    }
}

/// A browser started with its own empty profile, so nothing it reads or saves
/// touches the person's own.
struct Running {
    child: Child,
    profile: PathBuf,
}

impl Running {
    /// `is_chrome` passes the profile as Chrome expects. Stardust sets its own profile
    /// path, which the same switch would conflict with, so it finds it from `HOME`.
    fn start(binary: &Path, args: &[String], is_chrome: bool) -> Self {
        static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let run = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let profile =
            std::env::temp_dir().join(format!("stardust-perf-{}-{run}", std::process::id()));
        std::fs::create_dir_all(&profile).expect("Failed to create a temporary profile");
        let mut command = Command::new(binary);
        command.args(args);
        // The welcome page's animation isn't what's being measured. Chrome takes
        // the same switch.
        command.arg("--no-first-run");
        // With `HOME` moved, macOS has no keychain to store the cookie key in, and
        // shows a dialog that holds up loading. A stand-in keychain avoids it.
        if cfg!(target_os = "macos") {
            command.arg("--use-mock-keychain");
        }
        if is_chrome {
            command.arg(format!("--user-data-dir={}", profile.display()));
        }
        let child = command
            .env("XDG_DATA_HOME", &profile)
            .env("HOME", &profile)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to start the browser");
        Self { child, profile }
    }

    /// The browser and its helpers, which all name the profile on their command line.
    fn processes(&self) -> Vec<u32> {
        let profile = self.profile.to_string_lossy().into_owned();
        let mut pids: Vec<u32> = command_lines()
            .into_iter()
            .filter(|(_, args)| args.contains(&profile))
            .map(|(pid, _)| pid)
            .collect();
        if !pids.contains(&self.child.id()) {
            pids.push(self.child.id());
        }
        pids
    }

    fn memory_mb(&self) -> u64 {
        memory_kb(&self.processes()) / 1024
    }

    fn cpu_ms(&self) -> u64 {
        cpu_ms(&self.processes())
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let pids = self.processes();
        let _ = self.child.kill();
        let _ = self.child.wait();
        for pid in pids {
            let _ = Command::new("kill")
                .arg(pid.to_string())
                .stderr(Stdio::null())
                .status();
        }
        thread::sleep(Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&self.profile);
    }
}

fn measure_one_tab(bundle: &Path, binary: &Path) -> Vec<Measurement> {
    let size_mb = folder_size(bundle) / (1024 * 1024);
    let (urls, first_request) = serve_test_pages(1);
    let started = Instant::now();
    let browser = Running::start(binary, &urls, false);

    let Ok(_) = first_request.recv_timeout(STARTUP_TIMEOUT) else {
        drop(browser);
        eprintln!("The browser never requested the test page.");
        exit(1);
    };
    let startup_ms = started.elapsed().as_millis() as u64;

    thread::sleep(SETTLE);
    let ram_mb = browser.memory_mb();
    let cpu_before = browser.cpu_ms();
    thread::sleep(CPU_SAMPLE);
    let cpu_used = browser.cpu_ms().saturating_sub(cpu_before);
    let idle_cpu = cpu_used * 100 / CPU_SAMPLE.as_millis() as u64;

    vec![
        Measurement {
            name: "installed size",
            unit: "MB",
            value: size_mb,
            limit: INSTALLED_SIZE_MB,
        },
        Measurement {
            name: "startup to first request",
            unit: "ms",
            value: startup_ms,
            limit: STARTUP_MS,
        },
        Measurement {
            name: "RAM with one simple tab",
            unit: "MB",
            value: ram_mb,
            limit: ONE_TAB_RAM_MB,
        },
        Measurement {
            name: "idle CPU, % of one core",
            unit: "%",
            value: idle_cpu,
            limit: IDLE_CPU_PERCENT,
        },
    ]
}

/// Opens 20 tabs of a page with some weight to it, waits, and returns the memory used.
///
/// Stardust loads every tab given on its command line. Chrome delays background tabs
/// at startup, so its tabs open one at a time through its debugging port instead,
/// which loads each one as a person opening tabs would.
fn measure_twenty_tabs(binary: &Path, is_chrome: bool) -> u64 {
    let (urls, requests) = serve_test_pages(TAB_COUNT);
    let browser = if is_chrome {
        let args: Vec<String> = [
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-sync",
            "--remote-debugging-port=0",
            &urls[0],
        ]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
        let browser = Running::start(binary, &args, true);
        let port = wait_for_debugging_port(&browser.profile);
        for url in &urls[1..] {
            open_chrome_tab(port, url);
        }
        browser
    } else {
        Running::start(binary, &urls, false)
    };
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    let mut requested = std::collections::HashSet::new();
    while requested.len() < TAB_COUNT {
        let left = deadline.saturating_duration_since(Instant::now());
        match requests.recv_timeout(left) {
            Ok(path) => {
                requested.insert(path);
            }
            Err(_) => break,
        }
    }
    if requested.len() < TAB_COUNT {
        eprintln!(
            "{} only requested {} of {TAB_COUNT} pages.",
            binary.display(),
            requested.len()
        );
        exit(1);
    }
    thread::sleep(TWENTY_TAB_SETTLE);
    let profile = browser.profile.to_string_lossy().into_owned();
    let renderers = command_lines()
        .iter()
        .filter(|(_, args)| args.contains(&profile) && args.contains("--type=renderer"))
        .count();
    let memory = browser.memory_mb();
    println!(
        "{}: {memory} MB, {renderers} renderer processes, {TAB_COUNT} tabs",
        if is_chrome { "Chrome" } else { "Stardust" }
    );
    print_breakdown(&browser);
    memory
}

/// Shows where the memory goes, by kind of process.
fn print_breakdown(browser: &Running) {
    let lines = command_lines();
    let mut kinds: std::collections::BTreeMap<String, Vec<u64>> = Default::default();
    for pid in browser.processes() {
        let args = lines
            .iter()
            .find(|(other, _)| *other == pid)
            .map(|(_, args)| args.as_str())
            .unwrap_or_default();
        let kind = process_kind(args);
        kinds
            .entry(kind)
            .or_default()
            .push(memory_kb(&[pid]) / 1024);
    }
    for (kind, mut sizes) in kinds {
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        let total: u64 = sizes.iter().sum();
        let list: Vec<String> = sizes.iter().map(u64::to_string).collect();
        println!("  {kind:<24} {total:>5} MB  ({})", list.join(", "));
    }
}

/// The browser process, or a helper's `--type`, with its utility service if any.
fn process_kind(args: &str) -> String {
    let switch = |name: &str| {
        args.split_whitespace()
            .find_map(|arg| arg.strip_prefix(name))
            .map(str::to_owned)
    };
    match (switch("--type="), switch("--utility-sub-type=")) {
        (Some(kind), Some(service)) => {
            format!("{kind} {}", service.split('.').next().unwrap_or(""))
        }
        (Some(kind), None) => kind,
        (None, _) => "browser".to_owned(),
    }
}

/// Chrome writes the port it chose into the profile.
fn wait_for_debugging_port(profile: &Path) -> u16 {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(port) = std::fs::read_to_string(profile.join("DevToolsActivePort"))
            .ok()
            .and_then(|text| text.lines().next()?.trim().parse().ok())
        {
            return port;
        }
        thread::sleep(Duration::from_millis(200));
    }
    eprintln!("Chrome never opened its debugging port.");
    exit(1);
}

fn open_chrome_tab(port: u16, url: &str) {
    let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) else {
        return;
    };
    let _ = write!(
        stream,
        "PUT /json/new?{url} HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
    );
    // Chrome keeps the connection open, so read only as much as the reply says it has.
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut reply = Vec::new();
    let mut chunk = [0; 4096];
    while let Ok(read @ 1..) = stream.read(&mut chunk) {
        reply.extend_from_slice(&chunk[..read]);
        if reply_is_complete(&reply) {
            break;
        }
    }
}

fn reply_is_complete(reply: &[u8]) -> bool {
    let text = String::from_utf8_lossy(reply);
    let Some((head, body)) = text.split_once("\r\n\r\n") else {
        return false;
    };
    let length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        })
        .unwrap_or(0);
    body.len() >= length
}

/// Serves `count` pages on a local port, reporting when the first request arrives.
/// Each page builds a few thousand elements, so a tab costs about what a simple real
/// page does.
fn serve_test_pages(count: usize) -> (Vec<String>, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to open a local port");
    let port = listener
        .local_addr()
        .expect("Failed to read the port")
        .port();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        const BODY: &str = "<!doctype html><title>perf</title><h1>Performance test</h1>\
            <script>for(let i=0;i<3000;i++){const p=document.createElement('p');\
            p.textContent='Line '+i+' of a page with some text on it.';\
            document.body.append(p)}</script>";
        // Browsers open connections ahead of time and may leave them idle, so each
        // one gets its own thread rather than blocking the rest.
        for mut stream in listener.incoming().flatten() {
            let sender = sender.clone();
            thread::spawn(move || {
                let mut request = [0; 2048];
                let read = stream.read(&mut request).unwrap_or(0);
                if read == 0 {
                    return;
                }
                let request = String::from_utf8_lossy(&request[..read]);
                // Favicon requests don't count as a page loading.
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_owned();
                let host = request
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("host:")
                            .map(str::to_owned)
                    })
                    .unwrap_or_default();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{BODY}",
                    BODY.len()
                );
                if path == "/" {
                    let _ = sender.send(host.trim().to_owned());
                }
            });
        }
    });
    // Each tab is its own site, as real tabs usually are, so a browser can't share
    // one renderer process between them. Chromium sends any `.localhost` name here.
    let urls = if count == 1 {
        vec![format!("http://127.0.0.1:{port}/")]
    } else {
        (0..count)
            .map(|index| format!("http://tab{index}.localhost:{port}/"))
            .collect()
    };
    (urls, receiver)
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

/// Every process's ID and command line.
#[cfg(target_os = "linux")]
fn command_lines() -> Vec<(u32, String)> {
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            let args = std::fs::read(entry.path().join("cmdline")).ok()?;
            Some((pid, String::from_utf8_lossy(&args).replace('\0', " ")))
        })
        .collect()
}

#[cfg(not(target_os = "linux"))]
fn command_lines() -> Vec<(u32, String)> {
    let output = Command::new("ps")
        .args(["-axww", "-o", "pid=,args="])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    output
        .lines()
        .filter_map(|line| {
            let (pid, args) = line.trim_start().split_once(' ')?;
            Some((pid.parse().ok()?, args.to_owned()))
        })
        .collect()
}

/// Memory in use, with shared pages divided among the processes sharing them.
#[cfg(target_os = "linux")]
fn memory_kb(pids: &[u32]) -> u64 {
    pids.iter()
        .filter_map(|pid| std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok())
        .filter_map(|rollup| parse_pss_kb(&rollup))
        .sum()
}

/// Each process's footprint, the figure Activity Monitor shows as Memory.
#[cfg(not(target_os = "linux"))]
fn memory_kb(pids: &[u32]) -> u64 {
    let mut command = Command::new("footprint");
    command.args(["-f", "bytes", "--noCategories"]);
    for pid in pids {
        command.arg("-p").arg(pid.to_string());
    }
    let output = command
        .stderr(Stdio::null())
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    parse_footprint_bytes(&output) / 1024
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_pss_kb(rollup: &str) -> Option<u64> {
    rollup
        .lines()
        .find(|line| line.starts_with("Pss:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

#[cfg_attr(target_os = "linux", allow(dead_code))]
fn parse_footprint_bytes(output: &str) -> u64 {
    output
        .lines()
        .filter_map(|line| line.trim().strip_prefix("phys_footprint:"))
        .filter_map(|value| value.trim().strip_suffix(" B")?.parse::<u64>().ok())
        .sum()
}

/// CPU time used so far, user plus system.
#[cfg(target_os = "linux")]
fn cpu_ms(pids: &[u32]) -> u64 {
    let ticks: u64 = pids
        .iter()
        .filter_map(|pid| std::fs::read_to_string(format!("/proc/{pid}/stat")).ok())
        .filter_map(|stat| parse_cpu_ticks(&stat))
        .sum();
    ticks * 1000 / clock_ticks_per_second()
}

#[cfg(not(target_os = "linux"))]
fn cpu_ms(pids: &[u32]) -> u64 {
    if pids.is_empty() {
        return 0;
    }
    let list: Vec<String> = pids.iter().map(u32::to_string).collect();
    let output = Command::new("ps")
        .args(["-o", "time=", "-p", &list.join(",")])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    output.lines().filter_map(parse_ps_time_ms).sum()
}

/// User plus system time. The process name can contain spaces, so count from after it.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_cpu_ticks(stat: &str) -> Option<u64> {
    let after_name = stat.rsplit_once(')')?.1;
    let mut fields = after_name.split_whitespace().skip(11);
    Some(fields.next()?.parse::<u64>().ok()? + fields.next()?.parse::<u64>().ok()?)
}

/// Reads `ps` CPU time, such as `0:03.35` or `1:02:03.50`.
#[cfg_attr(target_os = "linux", allow(dead_code))]
fn parse_ps_time_ms(time: &str) -> Option<u64> {
    let mut seconds = 0.0;
    for part in time.trim().split(':') {
        seconds = seconds * 60.0 + part.parse::<f64>().ok()?;
    }
    Some((seconds * 1000.0) as u64)
}

#[cfg(target_os = "linux")]
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

    #[test]
    fn adds_up_footprints() {
        let output = "=====\nstardust [1]: 64-bit    Footprint: 9 B\n=====\n\nAuxiliary data:\n    \
            phys_footprint: 2048 B\n    phys_footprint_peak: 9999 B\n\n=====\n    phys_footprint: 1024 B\n";
        assert_eq!(parse_footprint_bytes(output), 3072);
    }

    #[test]
    fn knows_when_a_reply_has_arrived() {
        assert!(!reply_is_complete(b"HTTP/1.1 200 OK\r\nContent-Length:4"));
        assert!(!reply_is_complete(
            b"HTTP/1.1 200 OK\r\nContent-Length:4\r\n\r\nab"
        ));
        assert!(reply_is_complete(
            b"HTTP/1.1 200 OK\r\nContent-Length:4\r\n\r\nabcd"
        ));
        assert!(reply_is_complete(b"HTTP/1.1 200 OK\r\n\r\n"));
    }

    #[test]
    fn names_process_kinds() {
        assert_eq!(process_kind("/x/stardust --url=a"), "browser");
        assert_eq!(
            process_kind("/x/helper --type=renderer --lang=en"),
            "renderer"
        );
        assert_eq!(
            process_kind(
                "/x/helper --type=utility --utility-sub-type=network.mojom.NetworkService"
            ),
            "utility network"
        );
    }

    #[test]
    fn reads_ps_cpu_time() {
        assert_eq!(parse_ps_time_ms("0:03.35"), Some(3350));
        assert_eq!(parse_ps_time_ms(" 1:02:03.50"), Some(3_723_500));
        assert_eq!(parse_ps_time_ms("x"), None);
    }
}
