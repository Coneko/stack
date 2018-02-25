use indoc;
use regex;
use std;
use std::io::Read;
use tempfile;
use errors::*;

pub struct Changeset {
    title: String,
    message: Option<String>,
    pr: Option<u64>,
    dependencies: Vec<u64>,
    dependents: Vec<u64>,
}

impl Changeset {
    pub fn new_from_editor() -> Result<Changeset> {
        let mut tmpfile: tempfile::NamedTempFile =
            tempfile::NamedTempFile::new().chain_err(|| "Failed to create new temporary file.")?;
        let editor = std::env::var("VISUAL")
            .or(std::env::var("EDITOR").or_else(
                |_| -> std::result::Result<String, std::env::VarError> { Ok("vi".to_string()) },
            ))
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
        match rc.success() {
            true => {
                let mut buf: String = String::new();
                tmpfile.read_to_string(&mut buf).chain_err(|| {
                    format!(
                        "Could not read contents of temporary file '{}' opened with editor '{}'.",
                        tmpfile.path().to_string_lossy(),
                        editor
                    )
                })?;
                Self::new_from_string(buf)
            }
            false => match rc.code() {
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
            },
        }
    }

    pub fn new_from_string(string: String) -> Result<Changeset> {
        let lines = string.lines();
        let mut title: Option<String> = None;
        let mut message: Vec<String> = Vec::new();
        let mut pr: Option<u64> = None;
        let mut dependencies: Vec<u64> = Vec::new();
        let mut dependents: Vec<u64> = Vec::new();

        for line in lines {
            match line {
                x if x.starts_with("#") => continue,
                x if x.starts_with("Pull request:") => {
                    if pr.is_none() {
                        pr = Some(match x.parse::<u64>() {
                        Ok(y) => y,
                        Err(_) => bail!("Could not parse pull request number from 'Pull request' field: '{}'.", x),
                    })
                    }
                }
                x if x.starts_with("Depends on:") => (),
                _ => (),
            }
        }

        Ok(Changeset {
            title: "".to_owned(),
            message: None,
            pr: None,
            dependencies: Vec::new(),
            dependents: Vec::new(),
        })
    }

    fn parse_pull_request(string: &str, github_owner: &str, github_repo: &str) -> Result<u64> {
        let pattern = format!(
            r"^\s*(https://github.com/{}/{}/pull/|http://github.com/{0}/{1}/pull/|#)?(?P<pr_number>[0-9]+)\s*$",
            github_owner,
            github_repo,
        );
        let re: regex::Regex =
            regex::Regex::new(&pattern).chain_err(|| "Could not construct pull request regex.")?;
        let captures = re.captures(string).ok_or(format!(
            "Could not extract pull request number in 'Pull request' field: '{}'.",
            string
        ))?;
        let pr_number = captures
            .name("pr_number")
            .ok_or(format!(
                "Could not find pull request number in 'Pull request' field: '{}'.",
                string
            ))?
            .as_str();
        let pr_number = pr_number.parse::<u64>().chain_err(|| {
            format!(
                "Could not parse pull request number from 'Pull request' field: '{}'.",
                pr_number
            )
        })?;
        Ok(pr_number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_from_string_cannot_create_from_empty_string() {
        let result = Changeset::new_from_string("".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn new_from_string_can_create_from_string_without_pr_field() {
        let message = indoc!(
            "
            This is the title.

            This is the longer description of the commit.
            Dependencies: https://github.com/Coneko/stack/pull/1
            "
        );
        let result = Changeset::new_from_string(message.to_string());
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.title, "This is the title.");
    }

    #[test]
    fn new_from_string_can_create_from_string_with_pr_field() {
        let message = indoc!(
            "
            This is the title.

            This is the longer description of the commit.
            Pull request: https://github.com/Coneko/stack/pull/1
            "
        );
        let result = Changeset::new_from_string(message.to_string());
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.title, "This is the title.");
        assert!(result.pr.is_some());
        let pr = result.pr.unwrap();
        assert_eq!(pr, 1);
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
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn parse_pull_request_can_pr_reference() {
        let result = Changeset::parse_pull_request("#1", "Coneko", "stack");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn parse_pull_request_can_parse_https_url() {
        let result = Changeset::parse_pull_request(
            "https://github.com/Coneko/stack/pull/1",
            "Coneko",
            "stack",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn parse_pull_request_can_parse_http_url() {
        let result = Changeset::parse_pull_request(
            "http://github.com/Coneko/stack/pull/1",
            "Coneko",
            "stack",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }
}
