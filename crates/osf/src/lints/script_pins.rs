//! A literal command-token check, not a script interpreter. Its command
//! registry is shared by detection and the mandatory coverage statement.

#[derive(Clone, Copy)]
enum Kind {
    PythonPackage,
    JavaScriptPackage,
    RustPackage,
    Container,
}

impl Kind {
    /// Options that take a value in a separate token. Any other option is
    /// read as a switch, so an unknown option never hides the package.
    #[allow(clippy::too_many_lines)] // a data table, one list per kind
    fn value_flags(self) -> &'static [&'static str] {
        match self {
            Kind::PythonPackage => &[
                "-r",
                "--requirement",
                "-c",
                "--constraint",
                "-i",
                "--index-url",
                "--extra-index-url",
                "-e",
                "--editable",
                "-f",
                "--find-links",
                "--target",
                "--prefix",
                "--root",
                "--src",
                "--timeout",
                "--retries",
                "--proxy",
                "--cert",
                "--cache-dir",
                "--trusted-host",
                "--python",
                "--no-binary",
                "--only-binary",
                "--platform",
                "--abi",
                "--implementation",
                "--python-version",
                "--log",
                "--pip-args",
                "--suffix",
            ],
            Kind::JavaScriptPackage => &[
                "--prefix",
                "--cache",
                "--registry",
                "--loglevel",
                "--userconfig",
                "--workspace",
                "--fetch-retries",
                "--tag",
                "--save-prefix",
                "--omit",
                "--include",
                "--cwd",
                "--dir",
                "-C",
                "--filter",
            ],
            Kind::RustPackage => &[
                "--features",
                "--bin",
                "--example",
                "--root",
                "--registry",
                "--index",
                "--target",
                "--target-dir",
                "--jobs",
                "-j",
                "--git",
                "--branch",
                "--tag",
                "--rev",
                "--path",
                "--profile",
                "--config",
                "--manifest-path",
                "--message-format",
                "--color",
            ],
            Kind::Container => &[
                "--name",
                "--network",
                "--net",
                "--platform",
                "-e",
                "--env",
                "--env-file",
                "-v",
                "--volume",
                "-p",
                "--publish",
                "--entrypoint",
                "-w",
                "--workdir",
                "-u",
                "--user",
                "--pull",
                "--restart",
                "-m",
                "--memory",
                "--cpus",
                "--cpuset-cpus",
                "-l",
                "--label",
                "-h",
                "--hostname",
                "--add-host",
                "--device",
                "--cap-add",
                "--cap-drop",
                "--mount",
                "--tmpfs",
                "--ulimit",
                "--gpus",
                "--runtime",
                "--security-opt",
                "--shm-size",
                "--link",
                "--ipc",
                "--pid",
                "--uts",
                "--userns",
                "--cgroupns",
                "--dns",
                "--expose",
                "--ip",
                "--mac-address",
                "--log-driver",
                "--group-add",
                "-a",
                "--cidfile",
                "--stop-signal",
                "--stop-timeout",
                "--health-cmd",
            ],
        }
    }

    fn actions(self) -> &'static [&'static str] {
        match self {
            Kind::JavaScriptPackage => &["install", "i", "add"],
            Kind::Container => &["run", "pull", "create"],
            Kind::PythonPackage | Kind::RustPackage => &["install"],
        }
    }
}

const COMMANDS: &[(&str, Kind)] = &[
    ("pip", Kind::PythonPackage),
    ("pip3", Kind::PythonPackage),
    ("pipx", Kind::PythonPackage),
    ("npm", Kind::JavaScriptPackage),
    ("pnpm", Kind::JavaScriptPackage),
    ("yarn", Kind::JavaScriptPackage),
    ("cargo", Kind::RustPackage),
    ("docker", Kind::Container),
    ("podman", Kind::Container),
];

/// Interpreters that run pip as a module.
const PYTHON_RUNNERS: &[&str] = &["python", "python3", "py"];

/// Words that start a command without being part of it.
const LEADING_WORDS: &[&str] = &[
    "RUN", "then", "do", "else", "!", "-", "command", "exec", "time", "nohup", "env",
];

/// Image tags that move, so they do not pin a version.
const FLOATING_TAGS: &[&str] = &[
    "latest", "stable", "edge", "nightly", "lts", "main", "master", "current", "rolling",
];

#[must_use]
pub fn coverage() -> String {
    let names = COMMANDS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ");
    let tags = FLOATING_TAGS.join(", ");
    format!("script-pin coverage: literal install/add/run/pull/create commands for {names}, plus python -m pip, uv pip and uv tool. Reads a command after sudo, doas, env assignments, RUN, then and do. Splits a line on &&, ||, ; | and & and checks each command. Reads an option before or after the package; an option not in the known value-taking list is read as a switch, so the token after an unknown option that takes a value is read as a package. Flags a container image in docker or podman run, pull and create, in Dockerfile FROM and in image: lines when it has no tag or digest or carries a floating tag ({tags}); any other tag counts as pinned. Not covered: a command with variables, expansions, backticks or backslashes; a command split over several lines; language API calls; indirect installs; requirements-file contents. Skips obvious local paths, VCS URLs, unscoped owner/repo shorthands, and archive suffixes .whl, .zip, .tgz, .tar.gz, .tar.bz2, .tar.xz, .tar and .gz. Other local/VCS spellings may be treated as direct package names. A clean finding list is not a complete script audit.")
}

fn exact_semver(version: &str) -> bool {
    semver::Version::parse(version.strip_prefix('=').unwrap_or(version)).is_ok()
}

fn archive_source(arg: &str) -> bool {
    [
        ".whl", ".zip", ".tgz", ".tar.gz", ".tar.bz2", ".tar.xz", ".tar", ".gz",
    ]
    .iter()
    .any(|extension| arg.ends_with(extension))
}

fn direct_package(arg: &str, kind: Kind) -> bool {
    !arg.is_empty()
        && arg != "."
        && !arg.starts_with(['.', '/', '\\'])
        && !arg.contains("://")
        && !arg.starts_with("file:")
        && !arg.starts_with("git+")
        && !archive_source(arg)
        && !(matches!(kind, Kind::JavaScriptPackage)
            && !arg.starts_with('@')
            && arg.split('/').count() == 2)
}

/// An image with no tag or digest, or with a floating tag.
fn floating_image(image: &str) -> bool {
    if image.is_empty()
        || image == "scratch"
        || image.contains('@')
        || image.starts_with(['.', '/'])
        || image.contains("://")
    {
        return false;
    }
    let name = image.rsplit('/').next().unwrap_or(image);
    match name.split_once(':') {
        None => true,
        Some((_, tag)) => FLOATING_TAGS.contains(&tag.to_ascii_lowercase().as_str()),
    }
}

fn basename(command: &str) -> &str {
    command
        .rsplit('/')
        .next()
        .unwrap_or(command)
        .trim_end_matches(".exe")
}

fn split_flag(arg: &str) -> (&str, Option<&str>) {
    arg.split_once('=')
        .map_or((arg, None), |(flag, value)| (flag, Some(value)))
}

/// Cuts a line at every `&&`, `||`, `;`, `|` and `&` outside quotes. A
/// redirect such as `2>&1` or `&>` is not a cut.
fn split_commands(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for (i, &c) in chars.iter().enumerate() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            current.push(c);
            continue;
        }
        match c {
            '\'' | '"' => {
                quote = Some(c);
                current.push(c);
            }
            ';' | '|' => parts.push(std::mem::take(&mut current)),
            '&' => {
                let prev = i.checked_sub(1).and_then(|p| chars.get(p)).copied();
                let redirect = matches!(prev, Some('>' | '<')) || chars.get(i + 1) == Some(&'>');
                if redirect {
                    current.push(c);
                } else {
                    parts.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    parts.push(current);
    parts
}

fn is_assignment(token: &str) -> bool {
    token.split_once('=').is_some_and(|(name, _)| {
        name.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// The tokens after any prefix words, sudo with its options, and env
/// assignments.
fn after_prefix(tokens: &[String]) -> &[String] {
    let mut i = 0;
    while let Some(token) = tokens.get(i) {
        let t = token.as_str();
        if LEADING_WORDS.contains(&t) || is_assignment(t) || (t.ends_with(':') && t != "image:") {
            i += 1;
        } else if matches!(t, "sudo" | "doas") {
            i += 1;
            while let Some(option) = tokens.get(i).filter(|o| o.starts_with('-')) {
                let takes_value =
                    matches!(option.as_str(), "-u" | "-g" | "-h" | "-p" | "-C" | "-U");
                i += if takes_value { 2 } else { 1 };
            }
        } else {
            break;
        }
    }
    tokens.get(i..).unwrap_or_default()
}

/// The kind of command and the tokens after the command word. Python's
/// `-m pip`, `uv pip` and `uv tool` are read as pip and pipx.
fn command_kind(tokens: &[String]) -> Option<(Kind, &[String])> {
    let command = basename(tokens.first()?);
    if PYTHON_RUNNERS.contains(&command) {
        let at = tokens
            .iter()
            .position(|t| t == "-m")
            .filter(|&i| matches!(tokens.get(i + 1).map(String::as_str), Some("pip" | "pip3")))?;
        return Some((Kind::PythonPackage, tokens.get(at + 2..)?));
    }
    if command == "uv" {
        let rest = tokens.get(1..)?;
        let at = rest.iter().position(|t| !t.starts_with('-'))?;
        let word = rest.get(at)?;
        let after = rest.get(at + 1..)?;
        return matches!(word.as_str(), "pip" | "tool").then_some((Kind::PythonPackage, after));
    }
    let (_, kind) = COMMANDS.iter().find(|(name, _)| *name == command)?;
    Some((*kind, tokens.get(1..)?))
}

/// The tokens after the action word (`install`, `add`, `run`), skipping
/// options placed before it.
fn after_action(kind: Kind, rest: &[String]) -> Option<&[String]> {
    let mut i = 0;
    while let Some(arg) = rest.get(i) {
        if arg.starts_with('-') {
            let (flag, value) = split_flag(arg);
            let takes_next = value.is_none() && kind.value_flags().contains(&flag);
            i += if takes_next { 2 } else { 1 };
        } else if arg == "global" && matches!(kind, Kind::JavaScriptPackage) {
            i += 1;
        } else {
            let after = rest.get(i + 1..)?;
            return kind.actions().contains(&arg.as_str()).then_some(after);
        }
    }
    None
}

fn is_redirect(arg: &str) -> bool {
    let rest = arg.trim_start_matches(|c: char| c.is_ascii_digit());
    rest.starts_with(['>', '<']) || arg.starts_with("&>")
}

fn install_unpinned(kind: Kind, args: &[String]) -> bool {
    let mut packages = Vec::new();
    let mut version = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--" {
            packages.extend(args.map(String::as_str));
            break;
        }
        if is_redirect(arg) {
            break;
        }
        let (flag, value) = split_flag(arg);
        if matches!(kind, Kind::RustPackage) && matches!(flag, "--version" | "--vers") {
            version = value.or_else(|| args.next().map(String::as_str));
            continue;
        }
        if kind.value_flags().contains(&flag) {
            if value.is_none() {
                args.next();
            }
            continue;
        }
        if matches!(kind, Kind::PythonPackage) && arg.starts_with("-r") && arg.len() > 2 {
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        packages.push(arg.as_str());
        // Arguments after the image are the container's command, not images.
        if matches!(kind, Kind::Container) {
            break;
        }
    }
    packages
        .iter()
        .filter(|arg| direct_package(arg, kind))
        .any(|arg| match kind {
            Kind::PythonPackage => !arg.split_once("==").is_some_and(|(name, v)| {
                !name.is_empty()
                    && !v.is_empty()
                    && v.chars().all(|c| {
                        c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '!')
                    })
            }),
            Kind::JavaScriptPackage => !arg.rsplit_once('@').is_some_and(|(name, v)| {
                let version = v.strip_prefix('=').unwrap_or(v);
                let version = version.strip_prefix('v').unwrap_or(version);
                !name.is_empty() && semver::Version::parse(version).is_ok()
            }),
            Kind::RustPackage => !version
                .or_else(|| arg.rsplit_once('@').map(|(_, v)| v))
                .is_some_and(exact_semver),
            Kind::Container => floating_image(arg),
        })
}

/// A Dockerfile `FROM` line. A stage named earlier is not an image.
fn from_unpinned(args: &[String], stages: &mut Vec<String>) -> bool {
    let mut rest = args.iter().filter(|a| !a.starts_with("--"));
    let Some(image) = rest.next() else {
        return false;
    };
    let unpinned = !stages.contains(&image.to_ascii_lowercase()) && floating_image(image);
    if rest.next().is_some_and(|w| w.eq_ignore_ascii_case("as")) {
        if let Some(stage) = rest.next() {
            stages.push(stage.to_ascii_lowercase());
        }
    }
    unpinned
}

fn segment_unpinned(segment: &str, stages: &mut Vec<String>) -> bool {
    if segment.contains(['$', '%', '`', '\\']) {
        return false;
    }
    let Some(tokens) = shlex::split(segment) else {
        return false;
    };
    let tokens = after_prefix(&tokens);
    match tokens.first().map(String::as_str) {
        Some("FROM") => from_unpinned(tokens.get(1..).unwrap_or_default(), stages),
        Some("image:") => tokens.get(1).is_some_and(|image| floating_image(image)),
        Some(_) => command_kind(tokens)
            .and_then(|(kind, rest)| Some((kind, after_action(kind, rest)?)))
            .is_some_and(|(kind, args)| install_unpinned(kind, args)),
        None => false,
    }
}

fn line_unpinned(line: &str, stages: &mut Vec<String>) -> bool {
    if line.trim_start().starts_with('#') {
        return false;
    }
    let mut found = false;
    for segment in split_commands(line) {
        found |= segment_unpinned(&segment, stages);
    }
    found
}

/// Only the common literal token subset is inspected. No execution,
/// interpolation or shell-specific escape semantics are assumed.
#[must_use]
pub fn has_unpinned_install(line: &str) -> bool {
    line_unpinned(line, &mut Vec::new())
}

/// The 1-based numbers of the lines in `text` that hold an unpinned install.
/// A Dockerfile stage name declared with `AS` is not read as an image later.
#[must_use]
pub fn unpinned_line_numbers(text: &str) -> Vec<usize> {
    let mut stages = Vec::new();
    text.lines()
        .enumerate()
        .filter(|(_, line)| line_unpinned(line, &mut stages))
        .map(|(i, _)| i + 1)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(lines: &[&str]) {
        for line in lines {
            assert!(has_unpinned_install(line), "missed: {line}");
        }
    }

    fn none(lines: &[&str]) {
        for line in lines {
            assert!(!has_unpinned_install(line), "false positive: {line}");
        }
    }

    #[test]
    fn options_before_the_package_do_not_hide_it() {
        all(&[
            "pip install --no-cache-dir requests",
            "npm install -g --no-fund foo",
            "pip install --timeout 30 requests",
        ]);
        none(&[
            "pip install --no-cache-dir requests==2.31.0",
            "npm install -g --no-fund foo@1.2.3",
            "pip install --timeout 30 requests==2.31.0",
        ]);
    }

    #[test]
    fn options_after_the_package_do_not_hide_it() {
        all(&[
            "pip install requests --no-cache-dir",
            "npm install foo --no-fund",
        ]);
        none(&[
            "pip install requests==2.31.0 --no-cache-dir",
            "npm install foo@1.2.3 --no-fund",
        ]);
    }

    #[test]
    fn an_unknown_option_does_not_clear_the_line() {
        all(&[
            "pip install --made-up-flag requests",
            "npm install --made-up=1 foo",
            "cargo install --made-up tool",
        ]);
        none(&[
            "pip install --made-up-flag requests==1.0.0",
            "npm install --made-up=1 foo@1.0.0",
        ]);
    }

    #[test]
    fn a_command_after_a_prefix_is_found() {
        all(&[
            "sudo npm install -g x",
            "sudo -E pip install x",
            "RUN pip install x",
            "python -m pip install x",
            "python3 -m pip install --upgrade x",
            "uv pip install x",
            "uv tool install x",
            "pipx install x",
            "yarn add x",
            "yarn global add x",
            "PIP_NO_CACHE_DIR=1 pip install x",
            "- run: pip install x",
        ]);
        none(&[
            "sudo npm install -g x@1.0.0",
            "RUN pip install x==1.0",
            "python -m pip install x==1.0",
            "uv pip install x==1.0",
            "pipx install x==1.0",
            "yarn add x@1.0.0",
            "sudo apt install x",
            "python script.py",
            "uv run script.py",
        ]);
    }

    #[test]
    fn each_command_in_a_chained_line_is_checked() {
        all(&[
            "pip install requests && echo ok",
            "echo ok; npm install -g x",
            "cat list | sudo pip install requests",
            "pip install a==1.0 && npm install b",
            "pip install requests & wait",
            "if true; then pip install x; fi",
        ]);
        none(&[
            "pip install requests==2.31.0 && echo ok",
            "echo 'pip install x && y'",
            "pip install a==1.0 && npm install b@1.0.0",
            "pip install a==1.0 2>&1",
            "pip install a==1.0 &> log",
        ]);
    }

    #[test]
    fn a_container_image_with_no_tag_or_digest_is_found() {
        all(&[
            "docker run ubuntu",
            "docker run --rm -it ubuntu bash",
            "docker pull ubuntu",
            "podman create ubuntu",
            "FROM node",
            "FROM --platform=linux/amd64 node AS build",
            "image: foo",
            "docker run localhost:5000/img",
        ]);
        none(&[
            "docker run ubuntu:22.04",
            "docker run ubuntu@sha256:0123456789abcdef",
            "FROM node:22.4.1",
            "FROM scratch",
            "image: foo:1.2.3",
            "docker run localhost:5000/img:1.2",
            "docker run -v /a:/b ubuntu:22.04 ls",
        ]);
    }

    #[test]
    fn a_dockerfile_stage_name_is_not_read_as_an_image() {
        let text = "FROM node:22.4.1 AS build\nFROM build\nFROM node\n";
        assert_eq!(unpinned_line_numbers(text), vec![3]);
    }

    #[test]
    fn a_floating_container_tag_is_found() {
        all(&[
            "docker run ubuntu:latest",
            "docker run --pull=always img:latest",
            "docker run --pull always img:latest",
            "image: foo:latest",
            "FROM node:latest",
            "FROM node:stable",
        ]);
        none(&[
            "docker run --pull=always img:1.2.3",
            "docker run --pull always img:1.2.3",
            "FROM node:22.4.1",
        ]);
    }

    #[test]
    fn requirement_files_and_exact_pins_stay_clean() {
        none(&[
            "pip install -r requirements.txt",
            "pip install --requirement=requirements.txt",
            "pip install -rrequirements.txt",
            "pip install -e .",
            "pip3 install example-tool==1.2.3",
            "npm install @scope/example-tool@1.2.3",
            "cargo install example-tool --version 1.2.3 --locked",
            "npm install",
        ]);
        all(&["pip install -r requirements.txt example-tool"]);
    }

    #[test]
    fn coverage_states_each_form_it_reads() {
        let text = coverage();
        for phrase in [
            "pipx",
            "yarn",
            "python -m pip",
            "uv pip",
            "sudo",
            "RUN",
            "&&",
            "before or after the package",
            "no tag or digest",
            "latest",
            "FROM",
            "image:",
            "variables",
            "requirements-file contents",
        ] {
            assert!(text.contains(phrase), "coverage lacks {phrase}: {text}");
        }
    }
}
