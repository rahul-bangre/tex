//! Running tectonic and turning its output into structured diagnostics.

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    /// "error" | "warning" | "box"
    pub severity: String,
    /// Path relative to the project, if known.
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileResult {
    pub ok: bool,
    pub pdf: Option<String>,
    pub ms: u64,
    /// The single incremental pass left cross-references unsettled; run once more.
    pub needs_rerun: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub output: String,
}

pub fn find_tectonic() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin", "/usr/bin"] {
        dirs.push(PathBuf::from(d));
    }
    if let Some(h) = &home {
        dirs.push(h.join(".cargo/bin"));
        dirs.push(h.join(".local/bin"));
    }
    dirs.into_iter().map(|d| d.join("tectonic")).find(|p| p.is_file())
}

pub fn run(tectonic: &Path, project: &Path, main: &str, outdir: &Path, full: bool) -> CompileResult {
    let started = Instant::now();
    let _ = std::fs::create_dir_all(outdir);
    let stem = Path::new(main).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let aux = outdir.join(format!("{stem}.aux"));
    if full {
        // Start from a clean slate: stale intermediates are the usual cause of odd errors.
        if let Ok(rd) = std::fs::read_dir(outdir) {
            for e in rd.flatten() {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let incremental = aux.is_file();
    let mut res = run_once(tectonic, project, main, outdir, &stem, incremental);
    if incremental && res.diagnostics.iter().any(|d| d.severity == "error" && mentions_intermediate(d)) {
        // A corrupt or stale .aux/.toc broke the build; rebuild from scratch once.
        let _ = std::fs::remove_file(&aux);
        res = run_once(tectonic, project, main, outdir, &stem, false);
    }
    res.ms = started.elapsed().as_millis() as u64;
    res
}

fn mentions_intermediate(d: &Diagnostic) -> bool {
    let exts = [".aux", ".toc", ".out", ".bbl", ".lof", ".lot", ".nav", ".snm"];
    exts.iter().any(|e| d.file.as_deref().map(|f| f.ends_with(e)).unwrap_or(false) || d.message.contains(e))
}

/// One tectonic invocation. When `incremental`, run a single TeX pass that reads the
/// previous build's .aux/.toc/.bbl from `outdir`: ~3x faster than a full multi-pass build.
fn run_once(tectonic: &Path, project: &Path, main: &str, outdir: &Path, stem: &str, incremental: bool) -> CompileResult {
    let pdf = outdir.join(format!("{stem}.pdf"));
    let wall_start = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let mut cmd = Command::new(tectonic);
    cmd.current_dir(project)
        .arg(main)
        .args(["--synctex", "--keep-logs", "--keep-intermediates", "--color", "never"])
        .args(["-Z", "continue-on-errors"])
        .arg("--outdir")
        .arg(outdir)
        .stdin(Stdio::null());
    if incremental {
        cmd.args(["--reruns", "0"]).arg("-Z").arg(format!("search-path={}", outdir.display()));
    }
    let output = cmd.output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            return CompileResult {
                ok: false,
                pdf: None,
                ms: 0,
                needs_rerun: false,
                diagnostics: vec![Diagnostic {
                    severity: "error".into(),
                    file: None,
                    line: None,
                    message: format!("Could not start tectonic: {e}"),
                }],
                output: String::new(),
            }
        }
    };

    let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        text.push_str(&stdout);
    }

    let mut diagnostics = parse_stderr(&text, main);
    let log = std::fs::read_to_string(outdir.join(format!("{stem}.log"))).unwrap_or_default();
    diagnostics.extend(parse_log(&log, project, main));
    fill_lines_from_log(&mut diagnostics, &log, main);
    if diagnostics.iter().any(|d| d.severity == "error" && !is_engine_noise(&d.message)) {
        diagnostics.retain(|d| !is_engine_noise(&d.message));
    }
    let needs_rerun = incremental && log_wants_rerun(&log);
    if needs_rerun {
        // These resolve on the follow-up pass the UI schedules; don't flash them.
        diagnostics.retain(|d| !is_rerun_notice(&d.message));
    }
    let mut seen = HashSet::new();
    diagnostics.retain(|d| seen.insert((d.severity.clone(), d.file.clone(), d.line, d.message.clone())));
    diagnostics.sort_by_key(|d| match d.severity.as_str() {
        "error" => 0,
        "warning" => 1,
        _ => 2,
    });

    let produced = pdf.is_file() && crate::library::mtime_ms(&pdf) + 1000 >= wall_start;
    let has_errors = diagnostics.iter().any(|d| d.severity == "error");
    CompileResult {
        ok: produced && !has_errors && output.status.success(),
        pdf: produced.then(|| pdf.to_string_lossy().to_string()),
        ms: 0,
        needs_rerun,
        diagnostics,
        output: text,
    }
}

/// tectonic's own wrap-up lines, which add nothing once the real TeX error is shown.
fn is_engine_noise(m: &str) -> bool {
    m.starts_with("something bad happened inside")
        || m.contains("engine had an unrecoverable error")
        || m.starts_with("halted on potentially-recoverable error")
        || m == "Emergency stop"
        || m.starts_with("Emergency stop")
}

/// Errors reported without a location: find `! message` in the log and the `l.N` that follows.
fn fill_lines_from_log(diags: &mut [Diagnostic], log: &str, main: &str) {
    let lines: Vec<&str> = log.lines().collect();
    for d in diags.iter_mut().filter(|d| d.severity == "error" && d.line.is_none()) {
        let key = d.message.trim_end_matches('.');
        if key.len() < 6 {
            continue;
        }
        if let Some(i) = lines.iter().position(|l| l.starts_with('!') && l.contains(key)) {
            for l in lines.iter().skip(i + 1).take(12) {
                if let Some(rest) = l.strip_prefix("l.") {
                    let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if let Ok(n) = n.parse() {
                        d.line = Some(n);
                        d.file.get_or_insert_with(|| main.to_string());
                    }
                    break;
                }
            }
        }
    }
}

fn log_wants_rerun(log: &str) -> bool {
    log.contains("Rerun to get")
        || log.contains("Label(s) may have changed")
        || log.contains("Rerun LaTeX")
        || log.contains("has changed. Rerun")
        || log.contains("Please rerun LaTeX")
}

fn is_rerun_notice(m: &str) -> bool {
    m.contains("Rerun to get")
        || m.contains("Label(s) may have changed")
        || m.contains("Rerun LaTeX")
        || m.contains("Please rerun")
        || m.contains("There were undefined references")
        || m.contains("has changed. Rerun")
}

/// tectonic prints `error: file.tex:12: message` and similar on stderr.
pub fn parse_stderr(text: &str, _main: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for line in text.lines() {
        let (sev, rest) = if let Some(r) = line.strip_prefix("error: ") {
            ("error", r)
        } else if let Some(r) = line.strip_prefix("warning: ") {
            ("warning", r)
        } else {
            continue;
        };
        if rest.starts_with("errors were issued by the TeX engine")
            || rest.starts_with("warnings were issued by the TeX engine")
            || rest.contains("use --print and/or --keep-logs")
        {
            continue;
        }
        // Try `file:line: message`.
        let mut parsed = None;
        if let Some((file, after)) = rest.split_once(':') {
            if let Some((num, msg)) = after.split_once(':') {
                if let Ok(n) = num.trim().parse::<u32>() {
                    if !file.contains(' ') {
                        let message = msg.trim().trim_start_matches('!').trim().to_string();
                        let is_box = message.starts_with("Overfull \\") || message.starts_with("Underfull \\");
                        parsed = Some(Diagnostic {
                            severity: if is_box { "box".into() } else { sev.into() },
                            file: Some(normalize_rel(file)),
                            line: Some(n),
                            message,
                        });
                    }
                }
            }
        }
        out.push(parsed.unwrap_or_else(|| Diagnostic {
            severity: sev.into(),
            file: None,
            line: None,
            message: rest.trim().trim_start_matches('!').trim().to_string(),
        }));
    }
    out
}

fn normalize_rel(f: &str) -> String {
    f.trim_start_matches("./").to_string()
}

/// Extract warnings from a TeX log, tracking which file is open via parentheses.
pub fn parse_log(log: &str, project: &Path, main: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut stack: Vec<Option<String>> = Vec::new();
    let project_str = project.to_string_lossy().to_string();

    let current = |stack: &Vec<Option<String>>| -> String {
        stack.iter().rev().find_map(|f| f.clone()).unwrap_or_else(|| main.to_string())
    };

    let lines: Vec<&str> = log.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        let warning_kind = if line.starts_with("LaTeX Warning:") {
            Some(("warning", "LaTeX Warning:".len()))
        } else if line.starts_with("Package ") && line.contains(" Warning:") {
            line.find(" Warning:").map(|p| ("warning", p + " Warning:".len()))
        } else if line.starts_with("Class ") && line.contains(" Warning:") {
            line.find(" Warning:").map(|p| ("warning", p + " Warning:".len()))
        } else if line.starts_with("LaTeX Font Warning:") {
            Some(("box", "LaTeX Font Warning:".len()))
        } else {
            None
        };

        if let Some((sev, offset)) = warning_kind {
            // Gather continuation lines (until blank line).
            let mut msg = line[offset..].trim().to_string();
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim().is_empty() && j - i < 8 {
                let cont = lines[j].trim();
                // Package continuations look like "(hyperref)   text".
                let cont = if cont.starts_with('(') {
                    cont.split_once(')').map(|(_, r)| r.trim()).unwrap_or(cont)
                } else {
                    cont
                };
                msg.push(' ');
                msg.push_str(cont);
                j += 1;
            }
            let line_no = find_after(&msg, "input line ").or_else(|| find_after(&msg, "on line "));
            let message = msg
                .replace(" on input line", "")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let message = strip_line_suffix(&message);
            out.push(Diagnostic {
                severity: sev.into(),
                file: Some(current(&stack)),
                line: line_no,
                message,
            });
            i = j;
            continue;
        }

        if line.starts_with("Overfull \\") || line.starts_with("Underfull \\") {
            let line_no = find_after(line, "at lines ").or_else(|| find_after(line, "at line "));
            let msg = line
                .split(" in paragraph")
                .next()
                .unwrap_or(line)
                .split(" detected at line")
                .next()
                .unwrap_or(line)
                .split(" has occurred while")
                .next()
                .unwrap_or(line)
                .to_string();
            out.push(Diagnostic {
                severity: "box".into(),
                file: Some(current(&stack)),
                line: line_no,
                message: msg,
            });
            i += 1;
            continue;
        }

        // Track file open/close parentheses.
        let bytes: Vec<char> = line.chars().collect();
        let mut k = 0;
        while k < bytes.len() {
            match bytes[k] {
                '(' => {
                    let rest: String = bytes[k + 1..].iter().collect();
                    let token: String = rest
                        .chars()
                        .take_while(|c| !c.is_whitespace() && *c != '(' && *c != ')' && *c != '[' && *c != '{')
                        .collect();
                    let file = if looks_like_file(&token) {
                        let t = token.trim_start_matches("./");
                        if let Some(r) = t.strip_prefix(&project_str) {
                            Some(r.trim_start_matches('/').to_string())
                        } else if t.starts_with('/') || t.contains("texmf") || token.starts_with("tectonic") {
                            None
                        } else if project.join(t).is_file() {
                            Some(t.to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    stack.push(file);
                    k += 1 + token.chars().count();
                    continue;
                }
                ')' => {
                    stack.pop();
                }
                _ => {}
            }
            k += 1;
        }
        i += 1;
    }
    out
}

fn looks_like_file(t: &str) -> bool {
    if t.len() < 3 {
        return false;
    }
    let ext = t.rsplit('.').next().unwrap_or("");
    t.contains('.') && matches!(ext, "tex" | "sty" | "cls" | "bbl" | "aux" | "clo" | "cfg" | "def" | "fd" | "ltx" | "toc" | "out" | "lof" | "lot" | "ind" | "nav" | "snm" | "dtx" | "bst" | "ldf" | "code" | "lua")
}

fn find_after(s: &str, pat: &str) -> Option<u32> {
    let i = s.find(pat)?;
    let digits: String = s[i + pat.len()..].chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn strip_line_suffix(m: &str) -> String {
    let m = m.trim_end_matches('.');
    // Remove a trailing " <number>" left over from "on input line N".
    if let Some((head, tail)) = m.rsplit_once(' ') {
        if tail.chars().all(|c| c.is_ascii_digit()) && !tail.is_empty() {
            return head.trim_end().trim_end_matches(',').to_string();
        }
    }
    m.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr() {
        let d = parse_stderr(
            "note: Running TeX ...\nerror: main.tex:8: Undefined control sequence\nwarning: main.tex:3: Overfull \\hbox (1pt too wide) in paragraph\nerror: main.tex:9: ! LaTeX Error: File `x.sty' not found.\nwarning: errors were issued by the TeX engine, but were ignored; use --print and/or --keep-logs for details.\nerror: something broke\n",
            "main.tex",
        );
        assert_eq!(d.len(), 4);
        assert_eq!(d[0].line, Some(8));
        assert_eq!(d[0].file.as_deref(), Some("main.tex"));
        assert_eq!(d[0].message, "Undefined control sequence");
        assert_eq!(d[1].severity, "box");
        assert_eq!(d[2].message, "LaTeX Error: File `x.sty' not found.");
        assert_eq!(d[3].file, None);
    }

    #[test]
    fn log_warnings() {
        let log = "(main.tex\nLaTeX Warning: Reference `sec:x' on page 1 undefined on input line 9.\n\nOverfull \\hbox (12.0pt too wide) in paragraph at lines 20--22\n)";
        let tmp = std::env::temp_dir().join("tex-test-proj");
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("main.tex"), "").unwrap();
        let d = parse_log(log, &tmp, "main.tex");
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].line, Some(9));
        assert_eq!(d[0].message, "Reference `sec:x' on page 1 undefined");
        assert_eq!(d[1].severity, "box");
        assert_eq!(d[1].line, Some(20));
    }
}
