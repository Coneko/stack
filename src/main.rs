#![recursion_limit = "1024"]
extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate git2;
extern crate hubcaps;
#[macro_use]
extern crate log;
extern crate regex;
extern crate tokio_core;

mod errors {
    error_chain!{
        links {
            Hubcaps(::hubcaps::errors::Error, ::hubcaps::errors::ErrorKind);
        }
        foreign_links {
            Fmt(::std::fmt::Error);
            Io(::std::io::Error);
        }
    }
}

use errors::*;
use futures::Stream;

quick_main!(run);

fn run() -> Result<i32> {
    env_logger::init();

    let prog = std::env::current_exe()
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
    let repo = git2::Repository::discover(".")
        .chain_err(|| "Not a git repository (or any of the parent directories).")?;
    let origin = repo.find_remote("origin")
        .chain_err(|| "Could not find remote origin.")?;
    let re = regex::Regex::new(r"^git@github\.com:(?P<owner>[^/]+)/(?P<repo>.+)\.git$")
        .chain_err(|| "Could not construct regex.")?;
    debug!("Remote origin url: '{:?}'", origin.url());
    let captures = re.captures(origin.url().ok_or("Could not read remote origin url.")?)
        .ok_or("Could not extract Github repo from origin url.")?;
    let github_owner = captures
        .name("owner")
        .ok_or("Could not find github owner in origin url.")?
        .as_str();
    debug!("Github owner: '{}'", github_owner);
    let github_repo = captures
        .name("repo")
        .ok_or("Could not find github repo in origin url.")?
        .as_str();
    debug!("Github repo: '{}'", github_repo);
    let token =
        std::env::var("GITHUB_TOKEN").chain_err(|| "No GITHUB_TOKEN environment variable found.")?;

    let mut core = tokio_core::reactor::Core::new().chain_err(|| "Could not create new core.")?;
    let github = hubcaps::Github::new(
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        Some(hubcaps::Credentials::Token(token)),
        &core.handle(),
    );
    let repo = github.repo(github_owner, github_repo);
    let pulls = repo.pulls();
    core.run(
        pulls
            .iter(&Default::default())
            .for_each(|pull| Ok(println!("{:#?}", pull))),
    )?;

    Ok(0)
}
