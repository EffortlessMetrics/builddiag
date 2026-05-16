//! Hook snippet generation for builddiag.
//!
//! This crate generates deterministic snippets for:
//! - pre-commit local hook config
//! - standalone Git pre-commit shell hook
//! - Husky pre-commit script

/// Profile selection for generated hook commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookProfile {
    /// Open-source profile.
    Oss,
    /// Team profile.
    Team,
    /// Strict profile.
    Strict,
}

impl HookProfile {
    /// Returns the CLI string value for the profile.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Oss => "oss",
            Self::Team => "team",
            Self::Strict => "strict",
        }
    }
}

/// Rendering options for hook snippets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitHooksSpec {
    /// Profile applied to generated commands.
    pub profile: HookProfile,
    /// Generate a faster, lower-I/O hook command for local feedback.
    pub quick_fail: bool,
}

impl Default for InitHooksSpec {
    fn default() -> Self {
        Self {
            profile: HookProfile::Oss,
            quick_fail: false,
        }
    }
}

/// A rendered hook bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HooksBundle {
    /// `builddiag check ...` command used by all snippets.
    pub check_command: String,
    /// Snippet for `.pre-commit-config.yaml`.
    pub pre_commit_yaml_snippet: String,
    /// Script body for `.git/hooks/pre-commit`.
    pub shell_hook_script: String,
    /// Script body for `.husky/pre-commit`.
    pub husky_hook_script: String,
}

/// Render all hook snippets for the provided spec.
pub fn render_hooks(spec: InitHooksSpec) -> HooksBundle {
    let check_command = build_check_command(spec);
    HooksBundle {
        pre_commit_yaml_snippet: render_pre_commit_yaml(&check_command),
        shell_hook_script: render_shell_hook(&check_command),
        husky_hook_script: render_husky_hook(&check_command),
        check_command,
    }
}

/// Build the `builddiag check` command used in hook snippets.
pub fn build_check_command(spec: InitHooksSpec) -> String {
    let mut cmd = format!(
        "builddiag check --root . --profile {}",
        spec.profile.as_str()
    );
    if spec.quick_fail {
        cmd.push_str(
            " --diff-aware --no-cache --format diagnostics \
             --out .git/builddiag/report.json --md .git/builddiag/comment.md",
        );
    }
    cmd
}

fn render_pre_commit_yaml(command: &str) -> String {
    format!(
        "repos:\n  - repo: local\n    hooks:\n      - id: builddiag\n        name: builddiag\n        entry: {command}\n        language: system\n        pass_filenames: false\n        files: '(Cargo\\.toml|rust-toolchain(\\.toml)?|\\.builddiag\\.toml)$'\n"
    )
}

fn render_shell_hook(command: &str) -> String {
    format!(
        "#!/bin/sh\nset -eu\n\nif ! command -v builddiag >/dev/null 2>&1; then\n  echo \"builddiag: binary not found in PATH\" >&2\n  exit 1\nfi\n\n{command}\n"
    )
}

fn render_husky_hook(command: &str) -> String {
    format!("#!/usr/bin/env sh\n. \"$(dirname -- \"$0\")/_/husky.sh\"\n\n{command}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_check_command_uses_profile() {
        let cmd = build_check_command(InitHooksSpec {
            profile: HookProfile::Strict,
            quick_fail: false,
        });
        assert_eq!(cmd, "builddiag check --root . --profile strict");
    }

    #[test]
    fn build_check_command_quick_fail_adds_fast_flags() {
        let cmd = build_check_command(InitHooksSpec {
            profile: HookProfile::Team,
            quick_fail: true,
        });
        assert!(cmd.contains("--profile team"));
        assert!(cmd.contains("--diff-aware"));
        assert!(cmd.contains("--no-cache"));
        assert!(cmd.contains("--format diagnostics"));
        assert!(cmd.contains("--out .git/builddiag/report.json"));
    }

    #[test]
    fn render_hooks_is_deterministic() {
        let spec = InitHooksSpec {
            profile: HookProfile::Oss,
            quick_fail: true,
        };
        let first = render_hooks(spec);
        let second = render_hooks(spec);
        assert_eq!(first, second);
    }

    #[test]
    fn pre_commit_snippet_contains_hook_regex() {
        let bundle = render_hooks(InitHooksSpec::default());
        assert!(bundle.pre_commit_yaml_snippet.contains("Cargo\\.toml"));
        assert!(
            bundle
                .pre_commit_yaml_snippet
                .contains("rust-toolchain(\\.toml)?")
        );
    }

    #[test]
    fn shell_and_husky_scripts_start_with_shebang() {
        let bundle = render_hooks(InitHooksSpec::default());
        assert!(bundle.shell_hook_script.starts_with("#!/bin/sh"));
        assert!(bundle.husky_hook_script.starts_with("#!/usr/bin/env sh"));
    }
}
