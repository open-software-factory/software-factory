//! Provider-neutral validation for a sandbox spec and a command: every check
//! runs before any provider call, so a refusal never reaches a tool.

use crate::sandbox::{CommandSpec, Limits, Mount, SandboxError, SandboxSpec};

/// The largest accepted process count: 2^22.
const MAX_PROCESSES: u32 = 4_194_304;

/// Whether `image` is pinned: a valid reference with a digest or a non-`latest` tag.
#[must_use]
pub fn image_is_pinned(image: &str) -> bool {
    validate_image(image).is_ok()
}

/// One image rejection: the reason, then the image text it names.
fn rejected_image(reason: &str, image: &str) -> SandboxError {
    SandboxError::Rejected(format!("{reason}: {image}"))
}

/// Checks `image` as `name[:tag][@digest]` and requires it to be pinned.
///
/// # Errors
/// Returns [`SandboxError::Rejected`] for an empty, dashed, malformed or
/// unpinned image.
fn validate_image(image: &str) -> Result<(), SandboxError> {
    if image.is_empty() {
        return Err(rejected_image("the image must not be empty", image));
    }
    if image.starts_with('-') {
        return Err(rejected_image("the image must not start with '-'", image));
    }
    if !is_valid_reference(image) {
        return Err(rejected_image("the image is not a valid reference", image));
    }
    if is_pinned_reference(image) {
        return Ok(());
    }
    Err(rejected_image(
        "the image must be pinned by digest or a non-latest tag",
        image,
    ))
}

/// Whether `image` matches `name[:tag][@digest]`.
fn is_valid_reference(image: &str) -> bool {
    if image.matches('@').count() > 1 {
        return false;
    }
    let (reference, digest) = match image.split_once('@') {
        Some((reference, digest)) => (reference, Some(digest)),
        None => (image, None),
    };
    if digest.is_some_and(|digest| !is_digest(digest)) {
        return false;
    }
    let (name, tag) = split_tag(reference);
    tag.is_none_or(is_tag) && is_name(name)
}

/// Whether a valid `image` carries a digest or a non-`latest` tag.
fn is_pinned_reference(image: &str) -> bool {
    if image.contains('@') {
        return true;
    }
    split_tag(image).1.is_some_and(|tag| tag != "latest")
}

/// Splits `reference` into its name and optional tag: the last `:` after the
/// last `/` (or in the whole text when it holds no `/`); a `:` before the last
/// `/` is a registry port.
fn split_tag(reference: &str) -> (&str, Option<&str>) {
    let start = reference.rfind('/').map_or(0, |slash| slash + 1);
    let Some(colon) = reference.get(start..).and_then(|rest| rest.rfind(':')) else {
        return (reference, None);
    };
    let colon = start + colon;
    let name = reference.get(..colon).unwrap_or_default();
    let tag = reference.get(colon + 1..).unwrap_or_default();
    (name, Some(tag))
}

/// Whether `digest` is `sha256:` then exactly 64 lowercase hex characters.
fn is_digest(digest: &str) -> bool {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Whether `tag` is a letter, digit or `_`, then up to 127 more letters,
/// digits, `.`, `_` or `-`.
fn is_tag(tag: &str) -> bool {
    let mut bytes = tag.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() && first != b'_' {
        return false;
    }
    tag.len() <= 128
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Whether `name` is `[registry/]path`, with at least one path component.
fn is_name(name: &str) -> bool {
    let components: Vec<&str> = name.split('/').collect();
    let Some(first) = components.first().copied() else {
        return false;
    };
    let path_start = if components.len() > 1
        && (first.contains('.') || first.contains(':') || first == "localhost")
    {
        if !is_registry_host(first) {
            return false;
        }
        1
    } else {
        0
    };
    let Some(path) = components.get(path_start..) else {
        return false;
    };
    !path.is_empty() && path.iter().all(|part| is_path_component(part))
}

/// Whether `component` is `host` or `host:port`: dot-separated labels, then
/// one to five port digits.
fn is_registry_host(component: &str) -> bool {
    let (host, port) = match component.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (component, None),
    };
    if port.is_some_and(|port| !is_port(port)) {
        return false;
    }
    !host.is_empty() && host.split('.').all(is_host_label)
}

/// Whether `port` is one to five ASCII digits.
fn is_port(port: &str) -> bool {
    (1..=5).contains(&port.len()) && port.bytes().all(|byte| byte.is_ascii_digit())
}

/// Whether `label` starts and ends with a letter or digit and holds only
/// letters, digits and `-` between them.
fn is_host_label(label: &str) -> bool {
    let (Some(first), Some(last)) = (label.as_bytes().first(), label.as_bytes().last()) else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && last.is_ascii_alphanumeric()
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// Whether `part` is lowercase letters and digits joined by `.`, `_`, `__`
/// or one or more `-`.
fn is_path_component(part: &str) -> bool {
    let bytes = part.as_bytes();
    let mut index = 0;
    while bytes.get(index).copied().is_some_and(is_path_byte) {
        index += 1;
    }
    if index == 0 {
        return false;
    }
    while index < bytes.len() {
        match bytes.get(index) {
            Some(b'.') => index += 1,
            Some(b'-') => {
                while bytes.get(index) == Some(&b'-') {
                    index += 1;
                }
            }
            Some(b'_') => {
                index += 1;
                if bytes.get(index) == Some(&b'_') {
                    index += 1;
                }
            }
            _ => return false,
        }
        let token_start = index;
        while bytes.get(index).copied().is_some_and(is_path_byte) {
            index += 1;
        }
        if index == token_start {
            return false;
        }
    }
    true
}

/// Whether `byte` may appear in a path component word: a lowercase letter or digit.
fn is_path_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

/// Checks `spec` before any runner call.
///
/// # Errors
/// Returns [`SandboxError::Rejected`] for an invalid or unpinned image, an
/// empty, root, zero or malformed user, a relative workdir, a malformed name,
/// a bad mount, or a bad resource limit.
pub fn validate_spec(spec: &SandboxSpec) -> Result<(), SandboxError> {
    validate_image(&spec.image)?;
    validate_user(&spec.user)?;
    if !spec.workdir.starts_with('/') {
        return Err(SandboxError::Rejected(format!(
            "the workdir must be absolute: {}",
            spec.workdir
        )));
    }
    validate_name(&spec.name)?;
    for mount in &spec.mounts {
        validate_mount(mount)?;
    }
    validate_limits(&spec.limits)?;
    Ok(())
}

/// Checks the resource limits before any runner call.
fn validate_limits(limits: &Limits) -> Result<(), SandboxError> {
    if limits.max_processes == 0 || limits.max_processes > MAX_PROCESSES {
        return Err(SandboxError::Rejected(format!(
            "the max_processes must be between 1 and {MAX_PROCESSES}: {}",
            limits.max_processes
        )));
    }
    if !is_memory_limit(&limits.memory) {
        return Err(SandboxError::Rejected(format!(
            "the memory must be a positive size with an optional b, k, m or g suffix: {}",
            limits.memory
        )));
    }
    Ok(())
}

/// Whether `memory` is 1-12 digits without a leading zero, then an optional b, k, m or g.
fn is_memory_limit(memory: &str) -> bool {
    let digits = ["b", "k", "m", "g"]
        .iter()
        .find_map(|suffix| memory.strip_suffix(*suffix))
        .unwrap_or(memory);
    let mut bytes = digits.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    matches!(first, b'1'..=b'9') && digits.len() <= 12 && bytes.all(|byte| byte.is_ascii_digit())
}

/// Checks one user string as `name[:group]`, neither part root or zero.
fn validate_user(user: &str) -> Result<(), SandboxError> {
    if user.is_empty() {
        return Err(SandboxError::Rejected(
            "the user must not be empty".to_string(),
        ));
    }
    let mut parts = user.split(':');
    let Some(first) = parts.next() else {
        return Err(SandboxError::Rejected(format!(
            "each user part must not be empty: {user}"
        )));
    };
    validate_user_part(user, first)?;
    if let Some(second) = parts.next() {
        validate_user_part(user, second)?;
    }
    if parts.next().is_some() {
        return Err(SandboxError::Rejected(format!(
            "the user must hold at most one ':': {user}"
        )));
    }
    Ok(())
}

/// Checks one user part: a positive number without a leading zero, or a name.
fn validate_user_part(user: &str, part: &str) -> Result<(), SandboxError> {
    if part.is_empty() {
        return Err(SandboxError::Rejected(format!(
            "each user part must not be empty: {user}"
        )));
    }
    if part.bytes().all(|byte| byte.is_ascii_digit()) {
        return validate_user_number(user, part);
    }
    if !is_user_name(part) {
        return Err(SandboxError::Rejected(format!(
            "a user name must be a letter or '_' then letters, digits, '_' and '-': {user}"
        )));
    }
    if part == "root" {
        return Err(SandboxError::Rejected(format!(
            "the user must not be root: {user}"
        )));
    }
    Ok(())
}

/// Checks a numeric user part: no leading zero, a `u32`, and not zero.
fn validate_user_number(user: &str, part: &str) -> Result<(), SandboxError> {
    if part.len() > 1 && part.starts_with('0') {
        return Err(SandboxError::Rejected(format!(
            "a user number must not have a leading zero: {user}"
        )));
    }
    match part.parse::<u32>() {
        Ok(0) => Err(SandboxError::Rejected(format!(
            "the user must not be root: {user}"
        ))),
        Ok(_) => Ok(()),
        Err(_) => Err(SandboxError::Rejected(format!(
            "a user number must fit in a u32: {user}"
        ))),
    }
}

/// Whether `part` matches `[A-Za-z_][A-Za-z0-9_-]*` over bytes.
fn is_user_name(part: &str) -> bool {
    let mut bytes = part.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != b'_' {
        return false;
    }
    bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// Checks one sandbox name: non-empty, then letters, digits, `_`, `.` and `-`.
fn validate_name(name: &str) -> Result<(), SandboxError> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(SandboxError::Rejected(
            "the name must not be empty".to_string(),
        ));
    };
    if !first.is_ascii_alphanumeric() {
        return Err(SandboxError::Rejected(format!(
            "the name must start with an ASCII letter or digit: {name}"
        )));
    }
    if let Some(bad) = chars.find(|c| !(c.is_ascii_alphanumeric() || matches!(*c, '_' | '.' | '-')))
    {
        return Err(SandboxError::Rejected(format!(
            "the name may hold only ASCII letters, digits, '_', '.' and '-': {name} ({bad})"
        )));
    }
    Ok(())
}

/// Checks one mount's paths: a non-empty absolute host, an absolute sandbox,
/// no `..` component, and no comma or newline.
fn validate_mount(mount: &Mount) -> Result<(), SandboxError> {
    if mount.host_path.is_empty() {
        return Err(SandboxError::Rejected(
            "a mount host path must not be empty".to_string(),
        ));
    }
    if !mount.host_path.starts_with('/') {
        return Err(SandboxError::Rejected(format!(
            "a mount host path must be absolute: {}",
            mount.host_path
        )));
    }
    if !mount.sandbox_path.starts_with('/') {
        return Err(SandboxError::Rejected(format!(
            "a mount sandbox path must be absolute: {}",
            mount.sandbox_path
        )));
    }
    for path in [&mount.host_path, &mount.sandbox_path] {
        if path.split('/').any(|component| component == "..") {
            return Err(SandboxError::Rejected(format!(
                "a mount path must not hold a '..' component: {path}"
            )));
        }
    }
    for path in [&mount.host_path, &mount.sandbox_path] {
        if path.contains(',') || path.contains('\n') || path.contains('\r') {
            return Err(SandboxError::Rejected(format!(
                "a mount path must not hold a comma or a newline: {path}"
            )));
        }
    }
    Ok(())
}

/// Checks `command` before any runner call.
///
/// # Errors
/// Returns [`SandboxError::Rejected`] for an empty program, a relative workdir, a zero timeout, a malformed environment key, or an environment value with a NUL byte.
pub fn validate_command(command: &CommandSpec) -> Result<(), SandboxError> {
    if command.program.is_empty() {
        return Err(SandboxError::Rejected(
            "the program must not be empty".to_string(),
        ));
    }
    if let Some(workdir) = &command.workdir {
        if !workdir.starts_with('/') {
            return Err(SandboxError::Rejected(format!(
                "the workdir must be absolute: {workdir}"
            )));
        }
    }
    if command.timeout_secs == Some(0) {
        return Err(SandboxError::Rejected(
            "the timeout must be at least one second".to_string(),
        ));
    }
    for (key, value) in &command.env {
        if !is_env_key(key) {
            return Err(SandboxError::Rejected(format!(
                "an environment variable name is not valid: {key}"
            )));
        }
        if value.contains('\0') {
            return Err(SandboxError::Rejected(format!(
                "the environment variable {key} holds a NUL byte"
            )));
        }
    }
    Ok(())
}

/// Whether `key` matches `[A-Za-z_][A-Za-z0-9_]*`.
fn is_env_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != b'_' {
        return false;
    }
    bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::Network;
    use std::collections::BTreeMap;

    fn spec() -> SandboxSpec {
        SandboxSpec {
            name: "build".to_string(),
            image: "example/base:1".to_string(),
            user: "dev".to_string(),
            workdir: "/work".to_string(),
            network: Network::Isolated,
            mounts: Vec::new(),
            limits: Limits::default(),
        }
    }

    fn command() -> CommandSpec {
        CommandSpec {
            program: "run".to_string(),
            args: vec!["--fast".to_string()],
            workdir: None,
            timeout_secs: Some(30),
            env: BTreeMap::new(),
            stdin: None,
        }
    }

    #[test]
    fn image_is_pinned_accepts_digests_and_explicit_tags() {
        let hex = "a".repeat(64);
        let long_tag = "a".repeat(128);
        for pinned in [
            "example/base:1".to_string(),
            "example/base:1.2".to_string(),
            "registry:5000/example/base:1.2".to_string(),
            "localhost/a:1".to_string(),
            format!("example/base@sha256:{hex}"),
            format!("example/base:1@sha256:{hex}"),
            "ghcr.io/org/img:v1_2-rc.1".to_string(),
            format!("example/base:{long_tag}"),
        ] {
            assert!(image_is_pinned(&pinned), "{pinned} is pinned");
        }
    }

    #[test]
    fn image_is_pinned_rejects_bad_references_latest_and_untagged() {
        let short = "a".repeat(63);
        let upper = "A".repeat(64);
        let wrong = "z".repeat(64);
        let long_tag = "a".repeat(129);
        let hex = "a".repeat(64);
        for unpinned in [
            "--image=--user=0:1".to_string(),
            "--user=0:1".to_string(),
            "-x".to_string(),
            "-".to_string(),
            "sleep".to_string(),
            "example/base".to_string(),
            "example/base:latest".to_string(),
            format!("example/base@sha256:{short}"),
            format!("example/base@sha256:{upper}"),
            format!("example/base@sha256:{wrong}"),
            "Example/base:1".to_string(),
            "example/Base:1".to_string(),
            "example//base:1".to_string(),
            "/example:1".to_string(),
            "example/:1".to_string(),
            "example/base:".to_string(),
            "example/base:1:2".to_string(),
            "example/base:-1".to_string(),
            "example/base:.1".to_string(),
            format!("example/base:{long_tag}"),
            "example/base:1 --user=0:1".to_string(),
            "example/base:1\n".to_string(),
            " example/base:1".to_string(),
            "example base:1".to_string(),
            format!("example/base:1@sha256:{hex}@sha256:{hex}"),
            "alpine".to_string(),
            "alpine:".to_string(),
            "alpine:1:2".to_string(),
            "Alpine:1".to_string(),
            "registry:123456/a:1".to_string(),
            "-registry.io/a:1".to_string(),
            "a/.b:1".to_string(),
            "a/b.:1".to_string(),
            "a/b_:1".to_string(),
            String::new(),
        ] {
            assert!(!image_is_pinned(&unpinned), "{unpinned:?} is not pinned");
        }
    }

    #[test]
    fn validate_spec_accepts_every_valid_image() {
        let hex = "a".repeat(64);
        let long_tag = "a".repeat(128);
        for image in [
            "example/base:1".to_string(),
            "example/base:1.2".to_string(),
            "registry:5000/example/base:1.2".to_string(),
            "localhost/a:1".to_string(),
            format!("example/base@sha256:{hex}"),
            format!("example/base:1@sha256:{hex}"),
            "ghcr.io/org/img:v1_2-rc.1".to_string(),
            format!("example/base:{long_tag}"),
            "alpine:3.20".to_string(),
            "osf-devcontainer:pr151".to_string(),
            "registry:5000".to_string(),
            "localhost:5000/a:1".to_string(),
            format!("alpine@sha256:{hex}"),
        ] {
            let mut spec = spec();
            spec.image = image.clone();
            validate_spec(&spec).expect(&image);
        }
    }

    #[test]
    fn validate_spec_accepts_a_safe_user_without_a_call() {
        for user in ["1", "dev", "dev:dev", "1000:1000", "dev:100", "_svc"] {
            let mut spec = spec();
            spec.user = user.to_string();
            validate_spec(&spec).expect("the user is accepted");
        }
    }

    #[test]
    fn validate_spec_accepts_the_largest_process_limit() {
        let mut spec = spec();
        spec.limits.max_processes = 4_194_304;
        validate_spec(&spec).expect("the limit is accepted");
    }

    #[test]
    fn validate_spec_accepts_a_good_memory_limit() {
        for memory in ["1", "512m", "4g", "100000k", "7b", "123456789012"] {
            let mut spec = spec();
            spec.limits.memory = memory.to_string();
            validate_spec(&spec).expect("the memory is accepted");
        }
    }

    #[test]
    fn validate_command_accepts_good_env_keys() {
        for key in ["A", "_x", "API_KEY1"] {
            let mut command = command();
            command.env.insert(key.to_string(), "value".to_string());
            validate_command(&command).expect("the key is accepted");
        }
    }
}
