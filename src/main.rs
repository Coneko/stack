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
    let captures = re.captures(origin.url().ok_or("Could not read remote origin url.")?)
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
    let mut push_callbacks = git2::RemoteCallbacks::default();
    push_callbacks.credentials(
        |_url, username_from_url, allowed_types| match username_from_url {
            Some(username) => git2::Cred::ssh_key_from_agent(username),
            None => git2::Cred::username("git"),
        },
    );
    let mut push_options = git2::PushOptions::default();
    push_options.packbuilder_parallelism(0);
    push_options.remote_callbacks(push_callbacks);
    let pr_base_branch_name: &str = &format!(
        "{}{}{}",
        pr_branch_prefix,
        head_commit.id(),
        pr_base_branch_postfix
    );
    let pr_base_branch: git2::Branch = repo.branch(pr_base_branch_name, &parent, true)
        .chain_err(|| format!("Could not create branch at parent '{}'", parent.id()))?;
    origin
        .push(&[pr_base_branch_name], Option::Some(&mut push_options))
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
        .push(&[pr_head_branch_name], Option::Some(&mut push_options))
        .chain_err(|| "Couldn't push PR head branch.")?;

    Ok(0)
}
