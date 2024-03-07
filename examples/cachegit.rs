use std::path::Path;
use git2::Repository;
use pkgbuild::{GitSourceFragment, Pkgbuild, Source, SourceProtocol};
use url::Url;
use clap::Parser;

#[derive(Parser)]
struct Arg {
    #[arg(short, long)]
    /// Fetch all refs, instead of only those declared in sources
    allrefs: bool,
    #[arg(short, long)]
    /// Print a config to be used for 7Ji/git-mirrorer and early quit
    prconf: bool,
    #[clap(default_value_t)]
    /// The prefix of a 7Ji/git-mirrorer instance, e.g. http://gmr.lan
    gmr: String,
}

fn fetchspec_from_source(source: &Source, allrefs: bool) -> String {
    if let SourceProtocol::Git { fragment, signed: _ } = &source.protocol {
        if allrefs {
            "*".into()
        } else {
            if let Some(fragment) = fragment {
                match fragment {
                    GitSourceFragment::Branch(branch) => format!("heads/{}", branch),
                    GitSourceFragment::Commit(_) => "*".into(),
                    GitSourceFragment::Tag(tag) => format!("tags/{}", tag),
                }
            } else {
                "*".into()
            }
        }
    } else {
        String::new()
    }
}

fn open_or_create_repo<P: AsRef<Path>, S: AsRef<str>>(path: P, remote: S) -> Repository {
    match Repository::open_bare(&path) {
        Ok(repo) => repo,
        Err(e) => {
            eprintln!("Failed to open repo at '{}': {}", path.as_ref().display(), e);
            match Repository::init_bare(&path) {
                Ok(repo) => {
                    println!("Created repo at '{}'", path.as_ref().display());
                    if let Err(e) = repo.remote_with_fetch("origin", remote.as_ref(), "+refs/*:refs/*") {
                        eprintln!("Failed to create remote '{}': {}", remote.as_ref(), e);
                        panic!("Failed to create remote")
                    }
                    repo
                },
                Err(e) => {
                    eprintln!("Failed to create repo at '{}': {}", path.as_ref().display(), e);
                    panic!("Failed to create repo")
                },
            }
        },
    }
}

fn cache_source<S: Into<String>>(source: &Source, allrefs: bool, gmr: S) {
    let fetchspec = fetchspec_from_source(source, allrefs);
    if fetchspec.is_empty() { return }
    let repo = open_or_create_repo(&source.name, &source.url);
    let url = Url::parse(&source.url).expect("Failed to parse git source url");
    let mut gmr_url = gmr.into();
    if let Some(domain) = url.domain() {
        gmr_url.push('/');
        gmr_url.push_str(domain);
    }
    gmr_url.push_str(url.path());
    let mut remote = repo.remote_anonymous(&gmr_url).expect("Failed to create anonymous remote");
    println!("Caching git source '{}' from gmr '{}'", source.name, &gmr_url);
    remote.fetch(&[format!("+refs/{}:refs/{}", fetchspec, fetchspec)], None, None).expect("Failed to fetch from remote");
    for head in remote.list().expect("Failed to list remote heads") {
        if head.name() == "HEAD" {
            if let Some(target) = head.symref_target() {
                repo.set_head(target).expect("Failed to update local HEAD");
            }
            break
        }
    }
}

fn print_config(pkgbuild: &Pkgbuild) {
    let mut repos = Vec::new();
    for source_with_checksum in pkgbuild.sources_with_checksums() {
        let source = &source_with_checksum.source;
        if let SourceProtocol::Git { fragment: _, signed: _ } = source.protocol {
            let mut repo = source.url.clone();
            if ! source.url.ends_with(".git") {
                repo.push_str(".git")   
            }
            repos.push(repo)
        }
    }
    repos.sort_unstable();
    repos.dedup();
    println!("repos:");
    for repo in repos.iter() {
        println!("  - {}", repo)
    }
}

fn main() -> Result<(), &'static str> {
    let arg = Arg::parse();
    let pkgbuild = pkgbuild::parse_one(Some("PKGBUILD")).unwrap();
    if arg.prconf {
        print_config(&pkgbuild);
        return Ok(())
    }
    if arg.gmr.is_empty() {
        eprintln!("You must set gmr url!");
        return Err("No GMR url set");
    }
    for source_with_checksum in pkgbuild.sources_with_checksums() {
        cache_source(&source_with_checksum.source, arg.allrefs, &arg.gmr);
    }
    Ok(())
}