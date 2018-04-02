use errors::*;
use regex;
use std;
use std::io::Read;
use tempfile;

pub struct Changeset {
    pub title: String,
    pub message: Option<String>,
    pub branch: Option<String>,
    pub pr: Option<String>,
}

impl Changeset {
    const BRANCH_FIELD_LABEL: &'static str = "Branch name:";
    const PR_FIELD_LABEL: &'static str = "Pull request:";

    pub fn new_from_editor(github_owner: &str, github_repo: &str) -> Result<Changeset> {
        let mut tmpfile =
            tempfile::NamedTempFile::new().chain_err(|| "Failed to create new temporary file.")?;
        let editor = std::env::var("VISUAL")
            .or_else(|_| {
                std::env::var("EDITOR").or_else(
                    |_| -> std::result::Result<String, std::env::VarError> { Ok("vi".to_string()) },
                )
            })
            .unwrap();
        let rc = std::process::Command::new(&editor)
            .args(&[tmpfile.path()])
            .status()
            .chain_err(|| {
                format!(
                    "Could not open temporary file '{}' with editor '{}'.",
                    tmpfile.path().to_string_lossy(),
                    editor
                )
            })?;
        if rc.success() {
            let mut buf = String::new();
            tmpfile.read_to_string(&mut buf).chain_err(|| {
                format!(
                    "Could not read contents of temporary file '{}' opened with editor '{}'.",
                    tmpfile.path().to_string_lossy(),
                    editor
                )
            })?;
            Self::new_from_string(&buf, github_owner, github_repo)
        } else {
            match rc.code() {
                Some(code) => bail!(
                    "Editor '{}' exited with code '{}' after opening temporary file '{}'.",
                    editor,
                    code,
                    tmpfile.path().to_string_lossy()
                ),
                None => bail!(
                    "Editor '{}' terminated by signal after opening temporary file '{}'.",
                    editor,
                    tmpfile.path().to_string_lossy()
                ),
            }
        }
    }

    pub fn new_from_string(
        string: &str,
        github_owner: &str,
        github_repo: &str,
    ) -> Result<Changeset> {
        let lines = string.lines();
        let mut title = None;
        let mut message = Vec::<&str>::new();
        let mut branch = None;
        let mut pr = None;

        for line in lines {
            match line {
                x if x.is_empty() => continue,
                x if x.starts_with('#') => continue,
                x if x.starts_with(Self::BRANCH_FIELD_LABEL) => match branch {
                    Some(_) => bail!(
                        "Multiple 'Branch name' fields found in changeset description:\n{}",
                        string,
                    ),
                    None => branch = Some(x[Self::BRANCH_FIELD_LABEL.len()..].trim().to_string()),
                },
                x if x.starts_with(Self::PR_FIELD_LABEL) => match pr {
                    Some(_) => bail!(
                        "Multiple 'Pull request' fields found in changeset description:\n{}",
                        string,
                    ),
                    None => match Self::parse_pull_request(
                        &x[Self::PR_FIELD_LABEL.len()..],
                        github_owner,
                        github_repo,
                    ) {
                        Ok(pr_url) => pr = Some(pr_url),
                        Err(_) => bail!(
                            "Could not parse pull request number from 'Pull request' field: '{}'.",
                            x
                        ),
                    },
                },
                x => match title {
                    Some(_) => message.push(x),
                    None => title = Some(x),
                },
            }
        }

        let title = title
            .ok_or_else(|| {
                format!(
                    "Could not parse title from changeset description:\n{}",
                    string
                )
            })?
            .to_string();
        let message = if message.is_empty() {
            None
        } else {
            Some(message.join("\n"))
        };

        Ok(Changeset {
            title,
            message,
            branch,
            pr,
        })
    }

    fn parse_pull_request(string: &str, github_owner: &str, github_repo: &str) -> Result<String> {
        let pattern = format!(
            r"^\s*(https://github.com/{}/{}/pull/|http://github.com/{0}/{1}/pull/|#)?(?P<pr_number>[0-9]+)\s*$",
            github_owner,
            github_repo,
        );
        let re =
            regex::Regex::new(&pattern).chain_err(|| "Could not construct pull request regex.")?;
        let captures = re.captures(string).ok_or_else(|| {
            format!(
                "Could not extract pull request number in 'Pull request' field: '{}'.",
                string
            )
        })?;
        let pr_number = captures
            .name("pr_number")
            .ok_or_else(|| {
                format!(
                    "Could not find pull request number in 'Pull request' field: '{}'.",
                    string
                )
            })?
            .as_str();
        let pr_number = pr_number.parse::<u64>().chain_err(|| {
            format!(
                "Could not parse pull request number from 'Pull request' field: '{}'.",
                pr_number
            )
        })?;
        Ok(format!(
            "https://github.com/{}/{}/pull/{}",
            github_owner, github_repo, pr_number,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MESSAGE_FIXTURE: &str = indoc!(
        "

        # First comment
        This is the title.
        # Another comment

        This is the first line of the description.
        # This is a comment in the middle of the description
        This is the second line of the description.

        Branch name: hello

        Pull request: https://github.com/Coneko/stack/pull/4
        "
    );

    #[test]
    fn new_from_string_cannot_create_from_empty_string() {
        let result = Changeset::new_from_string("", "Coneko", "stack");
        assert!(result.is_err());
    }

    #[test]
    fn new_from_string_cannot_create_from_string_without_title() {
        let message = indoc!(
            "

            # comment
            Branch name: hello
            Pull request: https://github.com/Coneko/stack/pull/1
            "
        );
        let result = Changeset::new_from_string(message, "Coneko", "stack");
        assert!(result.is_err());
    }

    #[test]
    fn new_from_string_can_create_from_string_with_only_title() {
        let message = indoc!(
            "
            This is the title.
            "
        );
        let result = Changeset::new_from_string(message, "Coneko", "stack");
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.title, "This is the title.");
    }

    #[test]
    fn new_from_string_can_read_title() {
        let result = Changeset::new_from_string(MESSAGE_FIXTURE, "Coneko", "stack");
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.title, "This is the title.")
    }

    #[test]
    fn new_from_string_can_read_message() {
        let result = Changeset::new_from_string(MESSAGE_FIXTURE, "Coneko", "stack");
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.message.is_some());
        let message = result.message.unwrap();
        assert_eq!(
            message,
            "This is the first line of the description.\nThis is the second line of the description.",
        );
    }

    #[test]
    fn new_from_string_can_read_branch() {
        let result = Changeset::new_from_string(MESSAGE_FIXTURE, "Coneko", "stack");
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.branch.is_some());
        let branch = result.branch.unwrap();
        assert_eq!(branch, "hello".to_string());
    }

    #[test]
    fn new_from_string_can_read_pr() {
        let result = Changeset::new_from_string(MESSAGE_FIXTURE, "Coneko", "stack");
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.pr.is_some());
        let pr = result.pr.unwrap();
        assert_eq!(pr, "https://github.com/Coneko/stack/pull/4");
    }

    #[test]
    fn new_from_string_cannot_create_from_string_with_multiple_pr_fields() {
        let message = indoc!(
            "
            This is the title.

            Pull request: https://github.com/Coneko/stack/pull/1
            This is the longer description of the commit.
            Pull request: https://github.com/Coneko/stack/pull/1
            "
        );
        let result = Changeset::new_from_string(message, "Coneko", "stack");
        assert!(result.is_err());
        let result = result.err().unwrap();
        assert!(result.description().contains("Multiple"));
    }

    #[test]
    fn new_from_string_cannot_create_from_string_with_invalid_pr_field() {
        let message = indoc!(
            "
            This is the title.

            Pull request: hello
            "
        );
        let result = Changeset::new_from_string(message, "Coneko", "stack");
        assert!(result.is_err());
        let result = result.err().unwrap();
        assert!(result.description().contains("Could not parse"));
    }

    #[test]
    fn parse_pull_request_cannot_parse_pr_from_empty_string() {
        let result = Changeset::parse_pull_request("", "Coneko", "stack");
        assert!(result.is_err());
    }

    #[test]
    fn parse_pull_request_cannot_parse_invalid_pr_field() {
        let result = Changeset::parse_pull_request("not a valid PR reference", "Coneko", "stack");
        assert!(result.is_err());
    }

    #[test]
    fn parse_pull_request_can_parse_number() {
        let result = Changeset::parse_pull_request("1", "Coneko", "stack");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "https://github.com/Coneko/stack/pull/1");
    }

    #[test]
    fn parse_pull_request_can_parse_pr_reference() {
        let result = Changeset::parse_pull_request("#1", "Coneko", "stack");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "https://github.com/Coneko/stack/pull/1");
    }

    #[test]
    fn parse_pull_request_can_parse_https_url() {
        let result = Changeset::parse_pull_request(
            "https://github.com/Coneko/stack/pull/1",
            "Coneko",
            "stack",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "https://github.com/Coneko/stack/pull/1");
    }

    #[test]
    fn parse_pull_request_can_parse_http_url() {
        let result = Changeset::parse_pull_request(
            "http://github.com/Coneko/stack/pull/1",
            "Coneko",
            "stack",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "https://github.com/Coneko/stack/pull/1");
    }
}
