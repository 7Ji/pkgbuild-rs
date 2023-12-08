use std::path::Path;

mod child_ios;
pub mod error;
pub mod parser;
pub mod parser_options;
pub mod parser_script;
pub mod parser_script_builder;
pub mod pkgbuild;
#[cfg(feature = "serde")]
pub(crate) mod serde_optional_bytes_arrays;

pub use parser::Parser;
pub use parser_options::ParserOptions;
pub use parser_script::ParserScript;
pub use parser_script_builder::ParserScriptBuilder;
pub use pkgbuild::{
    UnorderedVersion, DependencyOrder, OrderedVersion,
    Dependency, Provide, Package, 
    Fragment, BzrSourceFragment, FossilSourceFragment, 
    GitSourceFragment, HgSourceFragment, SvnSourceFragment,
    SourceProtocol, Source, Cksum, Sha1sum, Sha224sum, 
    Sha256sum, Sha384sum, Sha512sum,
    Pkgbuild, Pkgbuilds};

use error::Result;

/// A shortcut to create a `Parser` and parse multiple `PKGBUILD`s
pub fn parse_multi<I, P>(paths: I) -> Result<Vec<Pkgbuild>>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>
{
    Parser::new()?.parse_multi(paths)
}

/// A shortcut to create a `Parser` and parse a single `PKGBUILD`
pub fn parse_one<P>(path: Option<P>) -> Result<Pkgbuild>
where
    P: AsRef<Path> 
{
    Parser::new()?.parse_one(path)
}