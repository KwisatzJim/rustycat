use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TEMP_DIR: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rustycat-tests-{}-{sequence}", std::process::id()));
        fs::create_dir(&path).expect("create test directory");
        Self(path)
    }

    fn file(&self, name: &str, contents: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, contents).expect("write test file");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn rcat() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rcat"))
}

#[test]
fn plain_mode_preserves_file_bytes() {
    let temp = TempDir::new();

    for (name, contents) in [
        ("no-newline.txt", b"no final newline".as_slice()),
        ("crlf.txt", b"first\r\nsecond\r\n".as_slice()),
        ("binary.bin", &[0xff, 0xfe, 0x00, 0x80]),
    ] {
        let path = temp.file(name, contents);
        let output = rcat()
            .args(["--plain", path.to_str().expect("UTF-8 test path")])
            .output()
            .expect("run rcat");

        assert!(output.status.success(), "rcat failed for {name}");
        assert_eq!(output.stdout, contents, "rcat changed {name}");
    }
}

#[test]
fn plain_mode_preserves_stdin_bytes() {
    let contents = [0xff, 0xfe, b'a', b'\r', b'\n'];
    let mut child = rcat()
        .arg("--plain")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start rcat");

    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(&contents)
        .expect("write stdin");

    let output = child.wait_with_output().expect("wait for rcat");
    assert!(output.status.success());
    assert_eq!(output.stdout, contents);
}

#[test]
fn numbering_continues_across_files() {
    let temp = TempDir::new();
    let first = temp.file("first.txt", b"one\ntwo\n");
    let second = temp.file("second.txt", b"three\nfour");

    let output = rcat()
        .args([
            "--plain",
            "--number",
            first.to_str().expect("UTF-8 test path"),
            second.to_str().expect("UTF-8 test path"),
        ])
        .output()
        .expect("run rcat");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("text output");
    assert!(stdout.contains("     1\tone\n     2\ttwo\n"));
    assert!(stdout.contains("     3\tthree\n     4\tfour"));
}

#[test]
fn missing_file_sets_failure_status() {
    let temp = TempDir::new();
    let missing = temp.0.join("missing.txt");

    let output = rcat()
        .args(["--plain", missing.to_str().expect("UTF-8 test path")])
        .output()
        .expect("run rcat");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing.txt"));
}

#[test]
fn dash_reads_stdin_between_files() {
    let temp = TempDir::new();
    let first = temp.file("first.txt", b"from file\n");
    let second = temp.file("second.txt", b"last file\n");
    let mut child = rcat()
        .args([
            "--plain",
            first.to_str().expect("UTF-8 test path"),
            "-",
            second.to_str().expect("UTF-8 test path"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start rcat");

    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(b"from stdin\n")
        .expect("write stdin");

    let output = child.wait_with_output().expect("wait for rcat");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("text output");
    assert!(stdout.contains("==> standard input <==\nfrom stdin\n"));
    assert!(stdout.find("from file").unwrap() < stdout.find("from stdin").unwrap());
    assert!(stdout.find("from stdin").unwrap() < stdout.find("last file").unwrap());
}

#[test]
fn color_option_controls_piped_output() {
    let temp = TempDir::new();
    let source = temp.file("example.rs", b"fn main() {}\n");

    let colored = rcat()
        .args([
            "--color",
            "always",
            source.to_str().expect("UTF-8 test path"),
        ])
        .output()
        .expect("run colored rcat");
    assert!(colored.status.success());
    assert!(colored.stdout.windows(2).any(|bytes| bytes == b"\x1b["));

    let plain = rcat()
        .args([
            "--color",
            "never",
            source.to_str().expect("UTF-8 test path"),
        ])
        .output()
        .expect("run plain rcat");
    assert!(plain.status.success());
    assert_eq!(plain.stdout, b"fn main() {}\n");
}
