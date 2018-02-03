#![recursion_limit = "1024"]
extern crate env_logger;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate git2;
extern crate hubcaps;
extern crate tokio_core;

mod errors {
    error_chain!{
        links {
            Hubcaps(::hubcaps::errors::Error, ::hubcaps::errors::ErrorKind);
        }
        foreign_links {
            Fmt(::std::fmt::Error);
            Io(::std::io::Error) #[cfg(unix)];
        }
    }
}

use errors::*;
use futures::Stream;

quick_main!(run);

fn run() -> Result<i32> {
    env_logger::init();
    let repo_root = std::env::args().nth(1).unwrap_or(".".to_string());
    let repo = git2::Repository::open(repo_root.as_str()).expect("Couldn't open repository");

    println!("{} state={:?}", repo.path().display(), repo.state());

    match std::env::var("GITHUB_TOKEN") {
        Ok(token) => {
            let mut core = tokio_core::reactor::Core::new()?;
            let github = hubcaps::Github::new(
                concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
                Some(hubcaps::Credentials::Token(token)),
                &core.handle(),
            );
            let repo = github.repo("softprops", "hubcat");
            let pulls = repo.pulls();
            core.run(
                pulls
                    .iter(&Default::default())
                    .for_each(|pull| Ok(println!("{:#?}", pull))),
            )?;

            println!("comments");
            for c in core.run(
                github
                    .repo("softprops", "hubcaps")
                    .pulls()
                    .get(28)
                    .comments()
                    .list(&Default::default()),
            )? {
                println!("{:#?}", c);
            }

            println!("commits");
            core.run(
                github
                    .repo("softprops", "hubcaps")
                    .pulls()
                    .get(28)
                    .commits()
                    .iter()
                    .for_each(|c| Ok(println!("{:#?}", c))),
            )?;
            Ok(0)
        }
        _ => Err("example missing GITHUB_TOKEN".into()),
    }
}
