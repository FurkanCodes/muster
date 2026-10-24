//! On macOS, `muster daemon --detach` has launchd start the daemon, so
//! quitting the terminal app it was started from leaves the daemon and its
//! sessions running (`PRD-09`, `ARCHITECTURE.md` §4).
//!
//! A program started from a terminal app stays in that app's launchd job,
//! and in its resource coalition, after `setsid` too; when the app quits,
//! macOS ends whatever is left there. So where the person is at the Mac's
//! own screen (not over SSH, where no terminal app is involved),
//! `--detach` has launchd run a job of its own in the person's `gui/<uid>`
//! domain, once. The job starts the daemon as `--detach` always has and
//! exits; the daemon stays in the job's coalition, which belongs to no app,
//! and the job is then removed, so no running job is left for a logout to
//! stop. Should launchd not run the job, or `launchctl` hang, the job is
//! removed all the same, the daemon is started directly, as before, and
//! why is logged.
//!
//! The job's environment is the starter's, byte for byte, written to a
//! file readable by the person alone in Muster's private runtime directory,
//! which the job reads and removes as it starts the daemon: the file gone
//! tells the starter the daemon is started. The job keeps the starter's
//! umask and open-files limit too, so agents create files and open as many
//! as they would have. `launchctl` is the program found on `PATH`, behind
//! [`Launchd`], so tests stand in for it (`ARC-03`).

use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{self, Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

/// The start of every job's label; the rest is random, so two starts never
/// meet.
const LABEL_PREFIX: &str = "muster.daemon.";

/// How long launchd may take to run the job before the daemon is started
/// directly instead. It takes a few milliseconds.
pub(crate) const STARTED_WITHIN: Duration = Duration::from_secs(3);

/// How often the starter looks whether the job has started the daemon.
const POLL: Duration = Duration::from_millis(20);

/// How long one `launchctl` may take before it is stopped. It answers in
/// milliseconds; one that hangs must not keep the daemon from starting.
/// The job given, waited for and removed, the starter gives up on launchd
/// well within the UI's [`muster_tui::START_AGAIN_AFTER`], so a UI never
/// starts a second starter while the first may still start the daemon.
const LAUNCHCTL_DEADLINE: Duration = Duration::from_secs(2);

/// What a run of `launchctl` came to.
#[derive(Debug)]
pub(crate) struct Answer {
    pub(crate) ok: bool,
    /// What it printed, for the log.
    pub(crate) said: String,
}

/// Where `launchctl` is reached: the program on `PATH`, or a test's
/// stand-in.
pub(crate) trait Launchd {
    /// Runs `launchctl` with `args`.
    fn launchctl(&self, args: &[OsString]) -> io::Result<Answer>;

    /// Waits `moment` before the starter looks again.
    fn pause(&self, moment: Duration);

    /// Says something in the log.
    fn note(&self, text: &str);
}

/// The person's own `launchctl`.
#[derive(Debug)]
pub(crate) struct System;

impl Launchd for System {
    fn launchctl(&self, args: &[OsString]) -> io::Result<Answer> {
        let mut child = Command::new("launchctl")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        // What it says fits in a pipe, so it is read once it has exited.
        let deadline = crate::instant_now() + LAUNCHCTL_DEADLINE;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if crate::instant_now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "launchctl had not answered after {} s",
                        LAUNCHCTL_DEADLINE.as_secs()
                    ),
                ));
            }
            #[expect(
                clippy::disallowed_methods,
                reason = "ARC-30: launchctl is waited on with a deadline, looked at every 5 ms"
            )]
            std::thread::sleep(Duration::from_millis(5));
        };
        let (mut out, mut err) = (String::new(), String::new());
        if let Some(mut pipe) = child.stdout.take() {
            let _ = pipe.read_to_string(&mut out);
        }
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut err);
        }
        let said = format!("{} {}", out.trim(), err.trim()).trim().to_owned();
        Ok(Answer {
            ok: status.success(),
            said,
        })
    }

    fn pause(&self, moment: Duration) {
        #[expect(
            clippy::disallowed_methods,
            reason = "ARC-30: launchd is given a moment between booting a job out and in again, which it asks for"
        )]
        std::thread::sleep(moment);
    }

    fn note(&self, text: &str) {
        note(text);
    }
}

/// Why the daemon was not started through launchd.
#[derive(Debug, thiserror::Error)]
pub(crate) enum LaunchError {
    #[error("could not write the job for launchd in {}: {source}", .dir.display())]
    Write { dir: PathBuf, source: io::Error },
    #[error("launchd cannot be given the path {}, which is not plain text", .0.display())]
    Path(PathBuf),
    #[error("could not run launchctl: {0}")]
    Run(io::Error),
    #[error("launchctl would not take the job: {0}")]
    Refused(String),
    #[error("launchd had not started the job after {} s", .0.as_secs())]
    NotStarted(Duration),
}

/// Whether `--detach` goes through launchd: on macOS, unless over SSH,
/// where no terminal app holds the session and the person's screen session
/// may not be the one at hand.
pub(crate) fn wanted(env: impl Fn(&str) -> Option<OsString>) -> bool {
    cfg!(target_os = "macos") && !over_ssh(env)
}

fn over_ssh(env: impl Fn(&str) -> Option<OsString>) -> bool {
    ["SSH_CONNECTION", "SSH_TTY"]
        .iter()
        .any(|key| env(key).is_some_and(|value| !value.is_empty()))
}

/// One start of the daemon through launchd.
#[derive(Debug)]
pub(crate) struct Job {
    /// Whose `gui/<uid>` domain it runs in.
    pub(crate) uid: u32,
    /// The muster the job runs: this one.
    pub(crate) program: PathBuf,
    /// Muster's private runtime directory, where the job's files are kept
    /// while it starts.
    pub(crate) runtime_dir: PathBuf,
    /// Where the job's errors and the daemon's log go.
    pub(crate) log: PathBuf,
    /// The umask the daemon is to have.
    pub(crate) umask: nix::sys::stat::Mode,
    /// The soft limit of open files the daemon is to have, where it has one.
    pub(crate) open_files: Option<u64>,
}

impl Job {
    /// A job for this process's muster, with its umask and limit, and its
    /// files in `runtime_dir`.
    pub(crate) fn for_this_process(runtime_dir: &Path) -> io::Result<Self> {
        let mask = nix::sys::stat::umask(nix::sys::stat::Mode::empty());
        nix::sys::stat::umask(mask);
        let open_files = nix::sys::resource::getrlimit(nix::sys::resource::Resource::RLIMIT_NOFILE)
            .ok()
            .map(|(soft, _)| soft)
            .filter(|soft| *soft != nix::sys::resource::RLIM_INFINITY);
        Ok(Self {
            uid: nix::unistd::getuid().as_raw(),
            program: std::env::current_exe()?,
            runtime_dir: runtime_dir.to_owned(),
            log: runtime_dir.join("daemon.log"),
            umask: mask,
            open_files,
        })
    }
}

/// Has launchd start the daemon through `job`, with `environment`. Returns
/// once the daemon is started, the job removed and its files gone.
pub(crate) fn start(
    launchd: &dyn Launchd,
    job: &Job,
    environment: impl IntoIterator<Item = (OsString, OsString)>,
) -> Result<(), LaunchError> {
    let dir = &job.runtime_dir;
    let write_error = |source| LaunchError::Write {
        dir: dir.clone(),
        source,
    };
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(write_error)?;
    // Made readable by the person alone, with a random name.
    let mut file = tempfile::Builder::new()
        .prefix("launch-")
        .suffix(".env")
        .rand_bytes(12)
        .tempfile_in(dir)
        .map_err(write_error)?;
    file.write_all(&encode(environment))
        .and_then(|()| file.flush())
        .map_err(write_error)?;
    let (_, carried) = file.keep().map_err(|error| write_error(error.error))?;
    let random = carried
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.strip_prefix("launch-"))
        .unwrap_or_default()
        .to_owned();
    let outcome = submit(launchd, job, &carried, &random);
    // Whatever came of it, no copy of the environment stays behind.
    let _ = std::fs::remove_file(&carried);
    outcome
}

/// Gives launchd the job that starts the daemon with the environment in
/// `carried`, waits until it has, and removes the job.
fn submit(
    launchd: &dyn Launchd,
    job: &Job,
    carried: &Path,
    random: &str,
) -> Result<(), LaunchError> {
    let label = format!("{LABEL_PREFIX}{random}");
    let definition = plist(job, &label, carried)?;
    let path = job.runtime_dir.join(format!("launch-{random}.plist"));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .and_then(|mut file| file.write_all(definition.as_bytes()))
        .map_err(|source| LaunchError::Write {
            dir: job.runtime_dir.clone(),
            source,
        })?;
    let domain = format!("gui/{}", job.uid);
    let given = launchd.launchctl(&[
        OsString::from("bootstrap"),
        OsString::from(&domain),
        path.clone().into_os_string(),
    ]);
    // launchd has read it, or will not.
    let _ = std::fs::remove_file(&path);
    // One that hung or failed may have loaded the job all the same: it is
    // removed, so it never starts a second daemon late.
    let failed = |error| {
        remove(launchd, &domain, &label, "may stay loaded");
        Err(error)
    };
    let given = match given {
        Ok(given) => given,
        Err(error) => return failed(LaunchError::Run(error)),
    };
    if !given.ok {
        return failed(LaunchError::Refused(given.said));
    }
    let mut started = !carried.exists();
    for _ in 0..STARTED_WITHIN.as_millis() / POLL.as_millis() {
        if started {
            break;
        }
        launchd.pause(POLL);
        started = !carried.exists();
    }
    if !started {
        // Not to do its work late.
        return failed(LaunchError::NotStarted(STARTED_WITHIN));
    }
    // The job has done its work.
    remove(
        launchd,
        &domain,
        &label,
        "that started the daemon stays, ended,",
    );
    Ok(())
}

/// Says something in the log, which is this process's standard error.
fn note(text: &str) {
    eprintln!("muster: {}", muster_core::text::multi_line(text));
}

/// Removes the job `label` from `domain`, saying in the log if it could not
/// and why: the job `stays` until the person logs out.
fn remove(launchd: &dyn Launchd, domain: &str, label: &str, stays: &str) {
    let removed = launchd.launchctl(&[
        OsString::from("bootout"),
        OsString::from(format!("{domain}/{label}")),
    ]);
    let why = match removed {
        Ok(answer) if answer.ok => return,
        Ok(Answer { said, .. }) => said,
        Err(error) => error.to_string(),
    };
    launchd.note(&format!(
        "the launchd job {label} {stays} until you log out, as launchctl could not remove it: {why}"
    ));
}

/// The job's definition, as launchd reads it (`launchd.plist(5)`): run once
/// as it is loaded and never again, its program left to go on when it
/// exits (the daemon is in a process group of its own then, but not yet as
/// it starts), with nothing held back as for a job in the background, and
/// as the starter would have run.
fn plist(job: &Job, label: &str, carried: &Path) -> Result<String, LaunchError> {
    let text = |path: &Path| {
        path.to_str()
            .and_then(xml_text)
            .ok_or_else(|| LaunchError::Path(path.to_owned()))
    };
    let (program, carried, log) = (text(&job.program)?, text(carried)?, text(&job.log)?);
    let label = xml_text(label).ok_or_else(|| LaunchError::Path(PathBuf::from(label)))?;
    let mut plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
<plist version=\"1.0\">
<dict>
\t<key>Label</key>
\t<string>{label}</string>
\t<key>ProgramArguments</key>
\t<array>
\t\t<string>{program}</string>
\t\t<string>daemon</string>
\t\t<string>--from-job</string>
\t\t<string>{carried}</string>
\t</array>
\t<key>RunAtLoad</key>
\t<true/>
\t<key>KeepAlive</key>
\t<false/>
\t<key>AbandonProcessGroup</key>
\t<true/>
\t<key>ProcessType</key>
\t<string>Interactive</string>
\t<key>Umask</key>
\t<integer>{umask}</integer>
\t<key>StandardInPath</key>
\t<string>/dev/null</string>
\t<key>StandardOutPath</key>
\t<string>/dev/null</string>
\t<key>StandardErrorPath</key>
\t<string>{log}</string>
",
        umask = job.umask.bits(),
    );
    if let Some(open_files) = job.open_files {
        let _ = write!(
            plist,
            "\t<key>SoftResourceLimits</key>
\t<dict>
\t\t<key>NumberOfFiles</key>
\t\t<integer>{open_files}</integer>
\t</dict>
"
        );
    }
    plist.push_str("</dict>\n</plist>\n");
    Ok(plist)
}

/// `text` as XML character data, or `None` if it holds a control
/// character, which XML cannot carry.
fn xml_text(text: &str) -> Option<String> {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            c if c.is_control() => return None,
            c => escaped.push(c),
        }
    }
    Some(escaped)
}

/// An environment as `NAME=value` entries, each ended by a NUL, which no
/// name or value can hold.
fn encode(environment: impl IntoIterator<Item = (OsString, OsString)>) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (name, value) in environment {
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(b'=');
        bytes.extend_from_slice(value.as_bytes());
        bytes.push(0);
    }
    bytes
}

/// The environment [`encode`] wrote. The name ends at the first `=` after
/// its first byte.
fn decode(bytes: &[u8]) -> Vec<(OsString, OsString)> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let at = entry.iter().skip(1).position(|byte| *byte == b'=')? + 1;
            Some((
                OsString::from_vec(entry[..at].to_vec()),
                OsString::from_vec(entry[at + 1..].to_vec()),
            ))
        })
        .collect()
}

/// What the job runs (`muster daemon --from-job <file>`): starts the daemon
/// apart from itself, as `--detach` does directly, with the environment in
/// `file`, then removes the file, which tells the starter the daemon is
/// started. Its errors go to the daemon's log.
pub(crate) fn run_job(file: &Path) -> ExitCode {
    let environment = match std::fs::read(file) {
        Ok(bytes) => decode(&bytes),
        Err(error) => {
            return crate::fail(format!(
                "could not read the daemon's environment from {}: {error}",
                file.display()
            ));
        }
    };
    let started = std::env::current_exe().and_then(|exe| {
        Command::new(exe)
            .args(["daemon", "--background"])
            .env_clear()
            .envs(environment)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
    });
    match started {
        // Not waited for: it runs on once this process has gone.
        Ok(_daemon) => {
            if let Err(error) = std::fs::remove_file(file) {
                note(&format!(
                    "could not remove the daemon's environment at {}: {error}",
                    file.display()
                ));
            }
            ExitCode::SUCCESS
        }
        // The file stays: the starter, not seeing it go, starts the daemon
        // directly and says why.
        Err(error) => crate::fail(format!("could not start the daemon: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// Stands in for `launchctl`: records each call, keeps the job's
    /// definition and environment as they were when launchd was given them,
    /// and, where `runs`, does what the job does on the first pause.
    #[derive(Debug, Default)]
    struct Fake {
        calls: RefCell<Vec<Vec<String>>>,
        refuses: bool,
        /// `bootstrap` hangs until it is stopped.
        hangs: bool,
        /// `bootout` fails.
        keeps: bool,
        runs: bool,
        notes: RefCell<Vec<String>>,
        pauses: Cell<u32>,
        given: RefCell<Option<(String, u32)>>,
        carried: RefCell<Option<(Vec<u8>, u32)>>,
    }

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    impl Launchd for Fake {
        fn launchctl(&self, args: &[OsString]) -> io::Result<Answer> {
            let args: Vec<String> = args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect();
            if args[0] == "bootstrap" {
                let path = Path::new(&args[2]);
                let text = std::fs::read_to_string(path).unwrap();
                let carried = text
                    .lines()
                    .skip_while(|line| !line.contains("--from-job"))
                    .nth(1)
                    .unwrap()
                    .trim()
                    .trim_start_matches("<string>")
                    .trim_end_matches("</string>")
                    .to_owned();
                *self.carried.borrow_mut() =
                    Some((std::fs::read(&carried).unwrap(), mode(Path::new(&carried))));
                *self.given.borrow_mut() = Some((text, mode(path)));
            }
            let bootout = args[0] == "bootout";
            self.calls.borrow_mut().push(args);
            if self.hangs && !bootout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "launchctl had not answered after 2 s",
                ));
            }
            if self.keeps && bootout {
                return Ok(Answer {
                    ok: false,
                    said: "Boot-out failed: 5: Input/output error".to_owned(),
                });
            }
            if self.refuses {
                return Ok(Answer {
                    ok: false,
                    said: "Bootstrap failed: 125: Domain does not support specified action"
                        .to_owned(),
                });
            }
            Ok(Answer {
                ok: true,
                said: String::new(),
            })
        }

        fn pause(&self, _moment: Duration) {
            self.pauses.set(self.pauses.get() + 1);
            if self.runs {
                let given = self.given.borrow();
                let text = &given.as_ref().unwrap().0;
                let carried = text
                    .lines()
                    .skip_while(|line| !line.contains("--from-job"))
                    .nth(1)
                    .unwrap()
                    .trim()
                    .trim_start_matches("<string>")
                    .trim_end_matches("</string>");
                let _ = std::fs::remove_file(carried);
            }
        }

        fn note(&self, text: &str) {
            self.notes.borrow_mut().push(text.to_owned());
        }
    }

    fn job(dir: &Path) -> Job {
        Job {
            uid: 501,
            program: PathBuf::from("/Users/a & b/.local/bin/muster"),
            runtime_dir: dir.join("run"),
            log: dir.join("run/daemon.log"),
            umask: nix::sys::stat::Mode::from_bits_truncate(0o077),
            open_files: Some(10_240),
        }
    }

    fn environment() -> Vec<(OsString, OsString)> {
        vec![
            (OsString::from("PATH"), OsString::from("/opt/bin:/usr/bin")),
            (OsString::from("EMPTY"), OsString::new()),
            (OsString::from("WITH_EQUALS"), OsString::from("a=b=c")),
            (
                OsString::from("NOT_TEXT"),
                OsString::from_vec(vec![0xff, b'x', 0xfe]),
            ),
        ]
    }

    /// What is left in the runtime directory.
    fn left(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir.join("run"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_job_goes_to_the_person_s_screen_session_and_is_removed_once_it_started_the_daemon_prd_09()
     {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake {
            runs: true,
            ..Fake::default()
        };
        start(&fake, &job(dir.path()), environment()).unwrap();

        let calls = fake.calls.borrow();
        assert_eq!(calls.len(), 2, "{calls:?}");
        assert_eq!(calls[0][..2], ["bootstrap", "gui/501"]);
        let plist = Path::new(&calls[0][2]);
        assert_eq!(plist.parent(), Some(dir.path().join("run").as_path()));
        let random = plist
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .strip_prefix("launch-")
            .unwrap()
            .strip_suffix(".plist")
            .unwrap();
        assert_eq!(random.len(), 12);
        let label = format!("gui/501/muster.daemon.{random}");
        assert_eq!(calls[1], ["bootout", label.as_str()]);

        // What launchd was given, readable by the person alone.
        let (text, plist_mode) = fake.given.borrow().clone().unwrap();
        assert_eq!(plist_mode, 0o600);
        for wanted in [
            format!("<string>muster.daemon.{random}</string>"),
            "<string>/Users/a &amp; b/.local/bin/muster</string>".to_owned(),
            "<string>daemon</string>".to_owned(),
            "<string>--from-job</string>".to_owned(),
            "<key>RunAtLoad</key>\n\t<true/>".to_owned(),
            "<key>KeepAlive</key>\n\t<false/>".to_owned(),
            "<key>AbandonProcessGroup</key>\n\t<true/>".to_owned(),
            "<key>ProcessType</key>\n\t<string>Interactive</string>".to_owned(),
            "<key>Umask</key>\n\t<integer>63</integer>".to_owned(),
            format!(
                "<key>StandardErrorPath</key>\n\t<string>{}</string>",
                dir.path().join("run/daemon.log").display()
            ),
            "<key>NumberOfFiles</key>\n\t\t<integer>10240</integer>".to_owned(),
        ] {
            assert!(text.contains(&wanted), "{wanted} in\n{text}");
        }
        // The environment, byte for byte, readable by the person alone.
        let (carried, carried_mode) = fake.carried.borrow().clone().unwrap();
        assert_eq!(carried_mode, 0o600);
        assert_eq!(decode(&carried), environment());
        // Nothing of the job is left.
        assert_eq!(left(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn a_job_launchd_will_not_take_leaves_nothing_behind_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake {
            refuses: true,
            ..Fake::default()
        };
        let error = start(&fake, &job(dir.path()), environment()).unwrap_err();
        assert!(matches!(error, LaunchError::Refused(_)), "{error}");
        assert!(
            error.to_string().contains("Domain does not support"),
            "{error}"
        );
        // Removed all the same, in case launchd loaded it.
        let calls = fake.calls.borrow();
        assert_eq!(calls.len(), 2, "{calls:?}");
        assert_eq!(calls[1][0], "bootout");
        assert_eq!(calls[1][1], calls[0][1].clone() + "/" + &label(&calls[0]));
        assert_eq!(left(dir.path()), Vec::<String>::new());
    }

    /// The label of the job `bootstrap` was given.
    fn label(bootstrap: &[String]) -> String {
        let plist = Path::new(&bootstrap[2])
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        let random = plist
            .strip_prefix("launch-")
            .unwrap()
            .strip_suffix(".plist")
            .unwrap();
        format!("{LABEL_PREFIX}{random}")
    }

    #[test]
    fn a_launchctl_that_hangs_is_stopped_and_its_job_removed_before_the_daemon_is_started_directly()
    {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake {
            hangs: true,
            ..Fake::default()
        };
        let error = start(&fake, &job(dir.path()), environment()).unwrap_err();
        assert!(
            matches!(&error, LaunchError::Run(error) if error.kind() == io::ErrorKind::TimedOut),
            "{error}"
        );
        let calls = fake.calls.borrow();
        assert_eq!(calls.len(), 2, "{calls:?}");
        assert_eq!(calls[0][0], "bootstrap");
        let removed = format!("gui/501/{}", label(&calls[0]));
        assert_eq!(calls[1], ["bootout", removed.as_str()]);
        // Not waited for: it never said it took the job.
        assert_eq!(fake.pauses.get(), 0);
        assert_eq!(fake.notes.borrow().len(), 0);
        assert_eq!(left(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn a_job_that_cannot_be_removed_is_logged_with_why() {
        for (hangs, runs) in [(true, false), (false, false), (false, true)] {
            let dir = tempfile::tempdir().unwrap();
            let fake = Fake {
                hangs,
                runs,
                keeps: true,
                ..Fake::default()
            };
            let started = start(&fake, &job(dir.path()), environment());
            assert_eq!(started.is_ok(), runs);
            let calls = fake.calls.borrow();
            assert_eq!(calls.last().unwrap()[0], "bootout", "{calls:?}");
            let notes = fake.notes.borrow();
            assert_eq!(notes.len(), 1, "{notes:?}");
            assert!(notes[0].contains(&label(&calls[0])), "{}", notes[0]);
            assert!(notes[0].contains("until you log out"), "{}", notes[0]);
            assert!(notes[0].contains("Input/output error"), "{}", notes[0]);
        }
    }

    #[test]
    fn the_starter_gives_up_on_launchd_before_the_ui_would_start_another() {
        // Bootstrap and bootout each stopped at their deadline, and the
        // wait for the job between them.
        let gives_up_within = LAUNCHCTL_DEADLINE * 2 + STARTED_WITHIN;
        // Then the daemon, started directly, opens its socket: well within
        // a few seconds.
        let direct_start = Duration::from_secs(3);
        assert!(
            gives_up_within + direct_start <= muster_tui::START_AGAIN_AFTER,
            "a starter may take {gives_up_within:?}, and the UI starts another after {:?}",
            muster_tui::START_AGAIN_AFTER
        );
    }

    #[test]
    fn a_job_launchd_never_runs_is_removed_before_the_daemon_is_started_directly() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        let error = start(&fake, &job(dir.path()), environment()).unwrap_err();
        assert!(matches!(error, LaunchError::NotStarted(_)), "{error}");
        let calls = fake.calls.borrow();
        assert_eq!(calls.len(), 2, "{calls:?}");
        assert_eq!(calls[1][0], "bootout");
        // It looked for as long as it gives launchd, and no longer.
        let polls = STARTED_WITHIN.as_millis() / POLL.as_millis();
        assert_eq!(u128::from(fake.pauses.get()), polls);
        assert_eq!(left(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn a_path_xml_cannot_carry_is_not_given_to_launchd() {
        let dir = tempfile::tempdir().unwrap();
        let mut job = job(dir.path());
        job.program = PathBuf::from("/bin/mus\u{1b}ter");
        let fake = Fake::default();
        let error = start(&fake, &job, environment()).unwrap_err();
        assert!(matches!(error, LaunchError::Path(_)), "{error}");
        assert_eq!(fake.calls.borrow().len(), 0);
        assert_eq!(left(dir.path()), Vec::<String>::new());
        assert_eq!(
            xml_text("<a href=\"x\">'&'</a>").unwrap(),
            "&lt;a href=&quot;x&quot;&gt;&apos;&amp;&apos;&lt;/a&gt;"
        );
    }

    #[test]
    fn without_an_open_files_limit_none_is_set() {
        let dir = tempfile::tempdir().unwrap();
        let mut job = job(dir.path());
        job.open_files = None;
        let text = plist(&job, "muster.daemon.x", Path::new("/run/launch-x.env")).unwrap();
        assert!(!text.contains("SoftResourceLimits"), "{text}");
    }

    #[test]
    fn the_environment_is_carried_byte_for_byte() {
        assert_eq!(decode(&encode(environment())), environment());
        // A name that starts with `=`, as some shells keep, stays whole.
        let odd = vec![(OsString::from("=C:"), OsString::from("x"))];
        assert_eq!(decode(&encode(odd.clone())), odd);
    }

    #[test]
    fn over_ssh_the_daemon_is_started_directly() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |key: &str| {
                pairs
                    .iter()
                    .find(|(name, _)| *name == key)
                    .map(|(_, value)| OsString::from(value))
            }
        };
        assert!(over_ssh(env(&[(
            "SSH_CONNECTION",
            "192.0.2.1 50000 192.0.2.2 22"
        )])));
        assert!(over_ssh(env(&[("SSH_TTY", "/dev/ttys004")])));
        assert!(!over_ssh(env(&[("SSH_CONNECTION", ""), ("SSH_TTY", "")])));
        assert!(!over_ssh(env(&[])));
        assert_eq!(wanted(env(&[])), cfg!(target_os = "macos"));
        assert!(!wanted(env(&[("SSH_TTY", "/dev/ttys004")])));
    }
}
