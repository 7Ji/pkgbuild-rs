
#[derive(Debug)]
pub enum Error {
    IoError(std::io::Error),
    NixErrno(nix::errno::Errno),
    /// The parsed result count is different from our input, but it might still
    /// be usable
    MismatchedResultCount {
        input: usize,
        output: usize,
        result: Vec<crate::pkgbuild::Pkgbuild>
    },
    /// The child's Stdio handles are incomplete and we can't get
    ChildStdioIncomplete,
    /// Some thread paniked and not joinable, this should not happen in our 
    /// code explicitly
    #[cfg(not(feature = "nothread"))]
    ThreadUnjoinable,
    /// Some PKGBUILDs were broken, this contains a list of those PKGBUILDs
    BrokenPKGBUILDs(Vec<String>),

    /// The parser script has returned some unexpected, illegal output
    ParserScriptIllegalOutput(Vec<u8>)
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::IoError(value)
    }
}

impl From<nix::errno::Errno> for Error {
    fn from(value: nix::errno::Errno) -> Self {
        Self::NixErrno(value)
    }
}