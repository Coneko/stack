#![feature(nll)]
#![recursion_limit = "1024"]
extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate git2;
extern crate hubcaps;
extern crate regex;
extern crate tokio_core;

mod errors {
    error_chain!{}
}

use errors::*;

quick_main!(run);

fn run() -> Result<i32> {
    env_logger::init();

    let prog: String = std::env::current_exe()
        .expect("Couldn't get program name.")
        .file_name()
        .expect("No file found.")
        .to_str()
        .expect("Not valid utf-8.")
        .to_owned();
    let matches = clap::App::new(prog)
        .about("Create stacked pull requests.")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand(clap::SubCommand::with_name("up").about("Uploads a commit in the stack."))
        .get_matches();

    match matches.subcommand_name() {
        Some("up") => run_up(),
        None => bail!("No subcommand specified."),
        _ => unreachable!(),
    }
}

fn run_up() -> Result<i32> {
    let pr_branch_prefix: String = format!(
        "{}-stack-",
        std::env::var("USER").chain_err(|| {
            "No USER environment variable found, cannot get current user's username."
        })?
    );
    let pr_head_branch_postfix = "-pr";
    let pr_base_branch_postfix = "-base";

    let repo: git2::Repository = git2::Repository::discover(".")
        .chain_err(|| "Not a git repository (or any of the parent directories).")?;
    let mut origin: git2::Remote = repo.find_remote("origin")
        .chain_err(|| "Could not find remote origin.")?;
    let re: regex::Regex = regex::Regex::new(
        r"^git@github\.com:(?P<owner>[^/]+)/(?P<repo>.+)\.git$",
    ).chain_err(|| "Could not construct regex.")?;
    let origin_url = origin.url().ok_or("Could not read remote origin url.")?;
    let captures = re.captures(origin_url)
        .ok_or("Could not extract Github repo from origin url.")?;
    let github_owner = captures
        .name("owner")
        .ok_or("Could not find github owner in origin url.")?
        .as_str();
    let github_repo = captures
        .name("repo")
        .ok_or("Could not find github repo in origin url.")?
        .as_str();
    let token: String =
        std::env::var("GITHUB_TOKEN").chain_err(|| "No GITHUB_TOKEN environment variable found.")?;

    let mut core: tokio_core::reactor::Core =
        tokio_core::reactor::Core::new().chain_err(|| "Could not create new core.")?;
    let github = hubcaps::Github::new(
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        Some(hubcaps::Credentials::Token(token)),
        &core.handle(),
    );
    let github_repo = github.repo(github_owner, github_repo);

    let head_commit: git2::Commit = repo.head()
        .chain_err(|| "Could not get HEAD reference.")?
        .peel_to_commit()
        .chain_err(|| "Could not get commit referenced by HEAD.")?;
    let mut parents = head_commit.parents();
    let parent = parents.next().ok_or("HEAD commit has no parents.")?;
    if parents.next().is_some() {
        bail!("HEAD commit has more than one parent.");
    }
    let repo_config = repo.config().chain_err(|| "Could not read repo config.")?;
    let mut push_options = push_options(origin_url, &repo_config);
    let pr_base_branch_name: &str = &format!(
        "{}{}{}",
        pr_branch_prefix,
        head_commit.id(),
        pr_base_branch_postfix
    );
    let pr_base_branch: git2::Branch = repo.branch(pr_base_branch_name, &parent, true)
        .chain_err(|| format!("Could not create branch at parent '{}'", parent.id()))?;
    origin
        .push(
            &[
                pr_base_branch.get().name().chain_err(|| {
                    format!(
                        "PR base branch '{}' has invalid reference name.",
                        pr_base_branch_name
                    )
                })?,
            ],
            Option::Some(&mut push_options),
        )
        .chain_err(|| "Couldn't push PR base branch.")?;
    let pr_head_branch_name: &str = &format!(
        "{}{}{}",
        pr_branch_prefix,
        head_commit.id(),
        pr_head_branch_postfix
    );
    let pr_head_branch: git2::Branch = repo.branch(pr_head_branch_name, &head_commit, false)
        .chain_err(|| format!("Could not create branch at head '{}'", head_commit.id()))?;
    origin
        .push(
            &[
                pr_head_branch.get().name().chain_err(|| {
                    format!(
                        "PR head branch '{}' has invalid reference name.",
                        pr_head_branch_name
                    )
                })?,
            ],
            Option::Some(&mut push_options),
        )
        .chain_err(|| "Couldn't push PR head branch.")?;
    Ok(0)
}

fn push_options<'a>(url: &str, config: &'a git2::Config) -> git2::PushOptions<'a> {
    let mut cred_helper = git2::CredentialHelper::new(url);
    cred_helper.config(config);
    let mut push_callbacks = git2::RemoteCallbacks::default();
    let mut tried_agent = false;
    push_callbacks.credentials(move |url, username_from_url, allowed_types| {
        if allowed_types.contains(git2::CredentialType::SSH_KEY) {
            let user = username_from_url
                .map(|s| s.to_string())
                .or_else(|| cred_helper.username.clone())
                .unwrap_or("git".to_string());
            if !tried_agent {
                tried_agent = true;
                git2::Cred::ssh_key_from_agent(&user)
            } else {
                match std::env::var("HOME") {
                    Ok(home) => git2::Cred::ssh_key(
                        &user,
                        None,
                        std::path::Path::new(&format!("{}/{}", home, ".ssh/id_rsa")),
                        None,
                    ),

                    Err(e) => Err(git2::Error::from_str(&format!(
                        "Could not get user home directory:\n{}",
                        e,
                    ))),
                }
            }
        } else if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            git2::Cred::credential_helper(config, url, username_from_url)
        } else if allowed_types.contains(git2::CredentialType::DEFAULT) {
            git2::Cred::default()
        } else {
            Err(git2::Error::from_str("no authentication available"))
        }
    });
    let mut push_options = git2::PushOptions::default();
    push_options.packbuilder_parallelism(0);
    push_options.remote_callbacks(push_callbacks);
    push_options
}
