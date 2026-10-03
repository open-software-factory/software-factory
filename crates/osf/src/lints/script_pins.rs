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
                "--target",
                "--prefix",
            ],
            Kind::JavaScriptPackage => &["--prefix", "--cache", "--registry"],
            Kind::RustPackage => &[
                "--features",
                "--bin",
                "--example",
                "--root",
                "--registry",
                "--target",
                "--jobs",
                "-j",
            ],
            Kind::Container => &[
                "--name",
                "--network",
                "--platform",
                "-e",
                "--env",
                "-v",
                "--volume",
                "-p",
                "--publish",
            ],
        }
    }
    fn switches(self) -> &'static [&'static str] {
        match self {
            Kind::PythonPackage => &[
                "--upgrade",
                "-U",
                "--user",
                "--no-deps",
                "--pre",
                "--quiet",
                "-q",
            ],
            Kind::JavaScriptPackage => &[
                "-g",
                "--global",
                "--save",
                "--save-dev",
                "-D",
                "--save-exact",
                "-E",
                "--ignore-scripts",
            ],
            Kind::RustPackage => &[
                "--locked",
                "--force",
                "--quiet",
                "-q",
                "--offline",
                "--frozen",
                "--list",
            ],
            Kind::Container => &[
                "--rm",
                "-d",
                "--detach",
                "-i",
                "-t",
                "-it",
                "--interactive",
                "--tty",
            ],
        }
    }
}

const COMMANDS: &[(&str, Kind)] = &[
    ("pip", Kind::PythonPackage),
    ("pip3", Kind::PythonPackage),
    ("npm", Kind::JavaScriptPackage),
    ("pnpm", Kind::JavaScriptPackage),
    ("cargo", Kind::RustPackage),
    ("docker", Kind::Container),
    ("podman", Kind::Container),
];

#[must_use]
pub fn coverage() -> String {
    let names = COMMANDS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ");
    format!("script-pin coverage: literal standalone install/add/run/pull commands for {names}; not a shell or language interpreter. Omits variables, expansions, escapes, pipelines, chained or multiline commands, language API calls, indirect installs, unknown command options and requirements-file contents. Skips obvious local paths, VCS URLs, unscoped owner/repo shorthands, and archive suffixes .whl, .zip, .tgz, .tar.gz, .tar.bz2, .tar.xz, .tar and .gz. Other local/VCS spellings may be treated as direct package names. A clean finding list is not a complete script audit.")
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

/// Only the common literal token subset is inspected. No execution,
/// interpolation or shell-specific escape semantics are assumed.
#[must_use]
pub fn has_unpinned_install(line: &str) -> bool {
    if line.trim_start().starts_with('#') || line.contains(['$', '%', '`', '\\', '|', '&', ';']) {
        return false;
    }
    let Some(tokens) = shlex::split(line) else {
        return false;
    };
    let Some(command) = tokens.first() else {
        return false;
    };
    let command = command
        .rsplit('/')
        .next()
        .unwrap_or(command)
        .trim_end_matches(".exe");
    let Some((_, kind)) = COMMANDS.iter().find(|(name, _)| *name == command) else {
        return false;
    };
    let Some(action) = tokens.get(1) else {
        return false;
    };
    let actions: &[&str] = match kind {
        Kind::JavaScriptPackage => &["install", "i", "add"],
        Kind::Container => &["run", "pull", "create"],
        Kind::PythonPackage | Kind::RustPackage => &["install"],
    };
    if !actions.contains(&action.as_str()) {
        return false;
    }
    let Some(args) = tokens.get(2..) else {
        return false;
    };
    let mut packages = Vec::new();
    let mut version = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--" {
            packages.extend(args.map(String::as_str));
            break;
        }
        let (flag, value) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(a, b)| (a, Some(b)));
        if matches!(kind, Kind::RustPackage) && matches!(flag, "--version" | "--vers") {
            version = value.or_else(|| args.next().map(String::as_str));
            continue;
        }
        let value_flags = kind.value_flags();
        if value_flags.contains(&flag) {
            if value.is_none() {
                args.next();
            }
            continue;
        }
        if matches!(kind, Kind::PythonPackage) && arg.starts_with("-r") && arg.len() > 2 {
            continue;
        }
        let switches = kind.switches();
        if arg.starts_with('-') {
            if !switches.contains(&arg.as_str()) {
                return false;
            }
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
        .filter(|arg| direct_package(arg, *kind))
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
            Kind::Container => arg.ends_with(":latest"),
        })
}
