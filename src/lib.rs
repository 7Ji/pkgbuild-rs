use std::{ffi::{OsString, OsStr}, path::{PathBuf, Path}, os::{unix::ffi::OsStrExt, fd::AsRawFd}, io::{Write, BufWriter, Read}, process::{Command, Stdio, Child, ChildStdin, ChildStdout, ChildStderr}, thread::spawn};

use libc::{PIPE_BUF, EAGAIN};
use nix::fcntl::{fcntl, FcntlArg::F_SETFL, OFlag};
use tempfile::NamedTempFile;

#[derive(Debug)]
pub enum Error {
    IoError(std::io::Error),
    NixErrno(nix::errno::Errno),
    /// The parsed result count is different from our input, but it might still
    /// be usable
    MismatchedResultCount {
        input: usize,
        output: usize,
        result: Vec<Pkgbuild>
    },
    /// The child's Stdio handles are incomplete and we can't get
    ChildStdioIncomplete,
    /// Some thread paniked and not joinable, this should not happen in our 
    /// code explicitly
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

/// The script builder to construct a `ParserScript` dynamically
pub struct ParserScriptBuilder {
    /// The path to makepkg library, usually `/usr/share/makepkg` on an Arch 
    /// installation
    pub makepkg_library: OsString,

    /// The makepkg configuration file, usually `/etc/makepkg.conf` on an Arch
    /// installation
    pub makepkg_config: OsString
}

/// Get a variable from environment, or use the default value if failed
fn env_or<K, O>(key: K, or: O) -> OsString 
where
    K: AsRef<OsStr>,
    O: Into<OsString>,
{
    std::env::var_os(key).unwrap_or(or.into())
}

impl Default for ParserScriptBuilder {
    fn default() -> Self {
        Self { 
            makepkg_library: env_or("LIBRARY", "/usr/share/makepkg"),
            makepkg_config: env_or("MAKEPKG_CONF", "/etc/makepkg.conf") 
        }
    }
}

impl ParserScriptBuilder {
    /// Create a new `ParserScriptBuilder` with `makepkg_library` and 
    /// `makepkg_config` initiailized with default values
    /// 
    /// `makepkg_library`: env `LIBRARY`, or default `/usr/share/makepkg`
    /// 
    /// `makepkg_config`: env `MAKEPKG_CONF`, or default `/etc/makepkg.conf`
    /// 
    /// Respective methods `set_makepkg_library()` and `set_makepkg_config()` 
    /// could be used to set these values to caller's fit
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the path to makepkg library, usually `/usr/share/makepkg` on an Arch 
    /// installation. 
    /// 
    /// If not set explicitly then the value of environment var `LIBRARY` (if 
    /// set), or the default value `/usr/share/makepkg` would be used.
    pub fn set_makepkg_library<O: Into<OsString>>(&mut self, library: O) 
        -> &mut Self 
    {
        self.makepkg_library = library.into();
        self
    }

    /// Set the path to the makepkg config, usually `/etc/makepkg.conf` on an
    /// Arch installation.
    /// 
    /// If not set explicitly then the value of environmenr var `MAKEPKG_CONF` (
    /// if set), or the default value `/etc/makepkg.conf` would be used
    pub fn set_makepkg_config<O: Into<OsString>>(&mut self, config: O) 
        -> &mut Self 
    {
        self.makepkg_config = config.into();
        self
    }

    /// Write the script content into the writer, this is an internal routine
    /// called by `build()` to wrap the `std::io::Result` type
    fn write<W>(&self, mut writer: BufWriter<W>) -> std::io::Result<()> 
    where
        W: Sized + Write
    {
        writer.write_all(b"LIBRARY='")?;
        writer.write_all(self.makepkg_library.as_bytes())?;
        writer.write_all(b"'\nMAKEPKG_CONF='")?;
        writer.write_all(self.makepkg_config.as_bytes())?;
        writer.write_all(b"'\n")?;
        writer.write_all(include_bytes!("parse_pkgbuild.bash"))
    }

    /// Build a `ParserScript`, would could later be used to parse `PKGBUILD`s
    /// 
    /// If `path` is `Some`, then create the file if not existing; if `path` is 
    /// `None`, then create a `NamedTempFile`. In both cases write the script
    /// dynamically generated into the file.
    /// 
    /// Return `Ok(ParserScript)` if write was successfull, return `Err` on IO
    /// Error.
    /// 
    /// To avoid any damage to possibly existing files, if we failed at
    /// `Some(path)`, we would not try to erase either the file or the content.
    /// Only when we failed at `None`, would the `NamedTempFile` be removed.
    pub fn build<P: AsRef<Path>>(&self, path: Option<P>) 
        -> Result<ParserScript> 
    {
        if let Some(path) = path {
            let file = match std::fs::File::create(&path) {
                Ok(file) => file,
                Err(e) => {
                    log::error!("Failed to create script file at '{}': {}",
                                    path.as_ref().display(), e);
                    return Err(e.into())
                },
            };
            if let Err(e) = self.write(
                BufWriter::new(file)) 
            {
                log::error!("Failed to write script into file '{}': {}", 
                     path.as_ref().display(), e);
                return Err(Error::IoError(e))
            }
            Ok(ParserScript::Persistent(path.as_ref().into()))
        } else {
            let mut temp_file = match 
                tempfile::Builder::new().prefix(".pkgbuild-rs").tempfile() 
            {
                Ok(temp_file) => temp_file,
                Err(e) => {
                    log::error!("Failed to create tempfile for script: {}", e);
                    return Err(e.into());
                },
            };
            if let Err(e) = self.write(
                BufWriter::new(temp_file.as_file_mut())) 
            {
                log::error!("Failed to write script into temp file '{}': {}", 
                     temp_file.path().display(), e);
                return Err(Error::IoError(e))
            }
            Ok(ParserScript::Temporary(temp_file))
        }
    }
}

pub enum ParserScript {
    Temporary(NamedTempFile),
    Persistent(PathBuf),
}

impl AsRef<OsStr> for ParserScript {
    fn as_ref(&self) -> &OsStr {
        match self {
            ParserScript::Temporary(temp_file) => 
                temp_file.path().as_os_str(),
            ParserScript::Persistent(path) => path.as_os_str(),
        }
    }
}

impl ParserScript {
    /// Generate a parser script at the given path, or create a named tempfile
    /// to store the script. 
    /// 
    /// This uses either `LIBRARY` from env or `/usr/share/makekg` if the env
    /// is missing for `makepkg_library` (named `LIBRARY` in  `makepkg` 
    /// routines) and either `MAKEPKG_CONF` from env or `/etc/makepkg.conf` if
    /// the env is missing for `makepkg_config` (named `MAKEPKG_CONF` in 
    /// `makepkg` routines). 
    /// 
    /// If customization on those variables are needed, then caller should 
    /// create a `ParserScript` with a `ParserScriptBuilder`
    pub fn new<P: AsRef<Path>>(path: Option<P>) -> Result<Self> {
        ParserScriptBuilder::new().build(path)
    }
}

/// Options used by `ParserScript` when parsing `PKGBUILD`s
pub struct ParserOptions {
    /// The interpreter used for the parser script, changing this only makes
    /// sense if you're working with a non-standard installation
    /// 
    /// Default: `/bin/bash`
    pub intepreter: PathBuf,

    /// Change the working directory before calling interpreter with the script
    /// 
    /// Default: `None`
    pub work_dir: Option<PathBuf>,

    /// Limit the parser implementation to only use a single thread. 
    /// 
    /// As we would feed the list of PKGBUILDs into the parser script's `stdin`,
    /// for minimum IO wait, when this is `false`, the library would spawn two 
    /// concurrent threads to write `stdin` and read `stderr`, while the main
    /// thread reads `stdout`
    /// 
    /// In some cases you might not want any thread to be spawned. Setting this
    /// to `true` would cause the library to use a dumber, page-by-page write+
    /// read behaviour in the same thread.
    /// 
    /// Default: `false`
    pub single_thread: bool,
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            intepreter: "/bin/bash".into(),
            work_dir: None,
            single_thread: false,
        }
    }
}

impl ParserOptions {
    /// Get a `ParserOptions` instance with default settings: no network, does
    /// not change work_dir
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the interpreter used for the `ParserScript`
    pub fn set_interpreter<P: Into<PathBuf>>(&mut self, interpreter: P)
    -> &mut Self
    {
        self.intepreter = interpreter.into();
        self
    }

    /// Set the work_dir to change to for the parser
    pub fn set_work_dir<P: Into<PathBuf>>(&mut self, work_dir: Option<P>)
    -> &mut Self
    {
        self.work_dir = work_dir.map(|path|path.into());
        self
    }
}

fn take_child_io<I>(from: &mut Option<I>) -> Result<I> {
    match from.take() {
        Some(taken) => Ok(taken),
        None => {
            log::error!("Failed to take Stdio handle from child");
            Err(Error::ChildStdioIncomplete)
        },
    }
}

fn set_nonblock<H: AsRawFd>(handle: &H) -> Result<()> {
    if let Err(e) = 
        fcntl(handle.as_raw_fd(), F_SETFL(OFlag::O_NONBLOCK)) 
    {
        log::error!("Failed to set IO handle as nonblock: {}", e);
        Err(e.into())
    } else {
        Ok(())
    }
}

struct ChildIOs {
    stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr
}

impl TryFrom<&mut Child> for ChildIOs {
    type Error = Error;

    fn try_from(child: &mut Child) -> Result<Self> {
        let stdin = take_child_io(&mut child.stdin)?;
        let stdout = take_child_io(&mut child.stdout)?;
        let stderr = take_child_io(&mut child.stderr)?;
        Ok(Self { stdin, stdout, stderr })
    }
}


impl ChildIOs {
    /// Set the underlying child stdin/out/err handles to non-blocking
    fn set_nonblock(&mut self) -> Result<()> {   
        set_nonblock(&self.stdin)?;
        set_nonblock(&self.stdout)?;
        set_nonblock(&self.stderr)
    }

    /// This is a sub-optimal single-thread implementation, extra times would
    /// be wasted on inefficient page-by-page try-reading to avoid jamming the
    /// child stdin/out/err.
    fn do_single_thread(mut self, input: &[u8]) -> Result<(Vec<u8>, Vec<u8>)>{
        self.set_nonblock()?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut buffer = vec![0; PIPE_BUF];
        let buffer = buffer.as_mut_slice();
        let mut written = 0;
        let total = input.len();
        let mut stdin_finish = false;
        let mut stdout_finish = false;
        let mut stderr_finish = false;
        // Rotate among stdin, stdout and stderr to avoid jamming
        loop {
            if ! stdin_finish {
                // Try to write at most the length of a PIPE buffer
                let mut end = written + libc::PIPE_BUF;
                if end > total {
                    end = total;
                }
                match self.stdin.write(&input[written..end]) {
                    Ok(written_this) => {
                        written += written_this;
                        if written >= total {
                            stdin_finish = true;
                            // drop(self.stdin)
                        }
                    },
                    Err(e) => 
                        if let Some(EAGAIN) = e.raw_os_error() {
                            log::warn!("Child stdin blocked")
                        } else {
                            log::error!("Failed to write to child-in: {}", e);
                            return Err(e.into())
                        },
                }
            }
            if ! stdout_finish {
                match self.stdout.read (&mut buffer[..]) {
                    Ok(read_this) =>
                        if read_this > 0 {
                            stdout.extend_from_slice(&buffer[0..read_this])
                        } else {
                            stdout_finish = true;
                            // drop(self.stdout)
                        },
                    Err(e) => 
                        if let Some(EAGAIN) = e.raw_os_error() {
                            log::warn!("Child stdout blocked")
                        } else {
                            log::error!("Failed to read from child-out: {}", e);
                            return Err(e.into())
                        },
                }
            }
            if ! stderr_finish {
                match self.stderr.read (&mut buffer[..]) {
                    Ok(read_this) =>
                        if read_this > 0 {
                            stderr.extend_from_slice(&buffer[0..read_this])
                        } else {
                            stderr_finish = true;
                            // drop(self.stderr)
                        }
                    Err(e) => 
                        if let Some(EAGAIN) = e.raw_os_error() {
                            log::warn!("Child stderr blocked")
                        } else {
                            log::error!("Failed to read from child-err: {}", e);
                            return Err(e.into())
                        },
                }
            }
            if stdin_finish && stdout_finish && stderr_finish {
                break
            }
        }
        Ok((stdout, stderr))
    }

    /// The multi-threaded 
    fn do_multi_thread(mut self, mut input: Vec<u8>) 
        -> Result<(Vec<u8>, Vec<u8>)> 
    {
        let stdin_writer = spawn(move||
            self.stdin.write_all(&mut input));
        let stderr_reader = spawn(move|| {
            let mut stderr = Vec::new();
            self.stderr.read_to_end(&mut stderr).and(Ok(stderr))
        });
        let mut last_error = None;
        let mut stdout = Vec::new();
        if let Err(e) = self.stdout.read_to_end(&mut stdout) {
            log::error!("Child stdout reader encountered IO error: {}", e);
            last_error = Some(e.into());
        }
        match stdin_writer.join() {
            Ok(writer_r) => if let Err(e) = writer_r {
                log::error!("Child stdin writer encountered IO error: {}", e);
                last_error = Some(e.into())
            },
            Err(_e) => 
                // This should not happend, but still covered anyway
                last_error = Some(Error::ThreadUnjoinable),
        }
        let stderr = match stderr_reader.join() {
            Ok(reader_r) => match reader_r {
                Ok(stderr) => stderr,
                Err(e) => {
                    log::error!("Child stderr reader encountered IO error: {}",
                                                                            e);
                    last_error = Some(e.into());
                    Vec::new()
                },
            },
            Err(_e) => {
                // This should not happend, but still covered anyway
                last_error = Some(Error::ThreadUnjoinable);
                Vec::new()
            }
        };
        // Now we're sure all threads are joined, safe to return error to caller
        if let Some(e) = last_error {
            Err(e)
        } else {
            Ok((stdout, stderr))
        }
    }
}

pub struct Parser {
    /// A on-disk or temporary file that stores the script that would be used
    /// to parse `PKGBUILD`s
    pub script: ParserScript,

    /// The options used when parsing `PKGBUILD`s
    pub options: ParserOptions,
}

impl Parser {
    /// Create a new parser with default settings
    pub fn new() -> Result<Self> {
        let script = ParserScript::new(None::<&str>)?;
        let options = ParserOptions::default();
        Ok(Self{
            script,
            options,
        })
    }

    /// Set the `ParserScript` instance used
    pub fn set_script(&mut self, script: ParserScript) -> &mut Self {
        self.script = script;
        self
    }

    /// Set the `ParserOptions` instance used
    pub fn set_options(&mut self, options: ParserOptions) -> &mut Self {
        self.options = options;
        self
    }

    /// Prepare a `Command` instance that could be used to spawn a `Child`
    fn get_command(&self) -> Command {
        let mut command = Command::new(
            &self.options.intepreter);
        command.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .arg(self.script.as_ref());
        if let Some(work_dir) = &self.options.work_dir {
            command.current_dir(work_dir);
        }
        command
    }

    /// Spawn a `Child` that's ready to parse `PKGBUILD`s
    fn get_child(&self) -> Result<Child> {
        self.get_command().spawn().map_err(|e|e.into())
    }

    /// Spawn a `Child` and take its `stdin`, `stdout`, `stderr` handles
    fn get_child_taken(&self) 
        -> Result<(Child, ChildIOs)> 
    {
        let mut child = self.get_child()?;
        let ios = ChildIOs::try_from(&mut child)?;
        Ok((child, ios))
    }

    /// Parse multiple PKGBUILD files
    pub fn parse_multi<I, P>(&self, paths: I) -> Result<Vec<Pkgbuild>>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>
    {
        let mut input = Vec::new();
        let mut count = 0;
        for path in paths {
            count += 1;
            input.extend_from_slice(path.as_ref().as_os_str().as_bytes());
            input.push(b'\n')
        }
        let (mut child, child_ios) = self.get_child_taken()?;
        // Do not handle the error yet, wait for the child to finish first
        let out_and_err = 
            if self.options.single_thread {
                child_ios.do_single_thread(&input)
            } else {
                child_ios.do_multi_thread(input)
            };
        let (out, err) = match out_and_err {
            Ok((out, err)) => {
                let status = match child.wait() {
                    Ok(status) => status,
                    Err(e) => {
                        log::error!("Failed to wait for child: {}", e);
                        return Err(e.into())
                    },
                };
                if ! status.success() {
                    log::error!("Child did not execute successfully");
                    return Err(Error::ChildStdioIncomplete)
                }
                (out, err)
            },
            Err(e) => {
                if let Err(e) = child.kill() {
                    log::error!("Failed to kill child after failed parsing");
                    return Err(e.into())
                }
                if let Err(e) = child.wait() {
                    log::error!("Failed to wait for killed child: {}", e);
                    return Err(e.into())
                }
                return Err(e)
            },
        };
        if ! err.is_empty() {
            log::warn!("Parser has written to stderr: \n{}", 
                String::from_utf8_lossy(&err));
        }
        if log::log_enabled!(log::Level::Debug) {
            log::debug!("Raw output from parser:\n{}", 
                String::from_utf8_lossy(&out));
        }
        let pkgbuilds = Pkgbuilds::from_borrowed(
            PkgbuildsParsing::from_parser_output(&out)?);
        let actual_count = pkgbuilds.entries.len();
        if actual_count != count {
            log::error!("Parsed PKGBUILDs count {} != input count {}",
                actual_count, count);
            return Err(Error::MismatchedResultCount { 
                input: count, output: actual_count, result: pkgbuilds.entries })
        }
        Ok(pkgbuilds.entries)

    }

    /// Parse only a single PKGBUILD file,
    /// 
    /// If `path` is `None`, defaults to `PKGBUILD`, i.e. parse the `PKGBUILD`
    /// in the work directory for parser. 
    pub fn parse_one<P>(&self, path: Option<P>) -> Result<Pkgbuild>
    where
        P: AsRef<Path> 
    {
        let mut pkgbuilds = match path {
            Some(path) => self.parse_multi(std::iter::once(path)),
            None => self.parse_multi(std::iter::once("PKGBUILD")),
        }?;
        let count = pkgbuilds.len();
        if count != 1 {
            log::error!("Parser return PKGBUILD count is not 1, but {}", count);
            return Err(Error::MismatchedResultCount { 
                input: 1, output: count, result: pkgbuilds })
        }
        match pkgbuilds.pop() {
            Some(pkgbuild) => Ok(pkgbuild),
            None => {
                // We should not be here
                log::error!("Parser returned no PKGBUILDs empty, it should be \
                    at least one");
                return Err(Error::MismatchedResultCount { 
                    input: 1, output: 0, result: pkgbuilds })
            },
        }
    }
}

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

/// A sub-package parsed from a split-package `PKGBUILD`, borrowed variant
/// during parsing. Library users should not used this.
struct PackageParsing<'a> {
    name: &'a [u8],
    deps: Vec<&'a [u8]>,
    provides: Vec<&'a [u8]>,
}

impl<'a> Default for PackageParsing<'a> {
    fn default() -> Self {
        Self {
            name: b"",
            deps: vec![],
            provides: vec![]
        }
    }
}

/// A `PKGBUILD` being parsed. Library users should
/// not use this.
struct PkgbuildParsing<'a> {
    base: &'a [u8],
    pkgs: Vec<PackageParsing<'a>>,
    deps: Vec<&'a [u8]>,
    makedeps: Vec<&'a [u8]>,
    provides: Vec<&'a [u8]>,
    sources: Vec<&'a [u8]>,
    cksums: Vec<&'a [u8]>,
    md5sums: Vec<&'a [u8]>,
    sha1sums: Vec<&'a [u8]>,
    sha224sums: Vec<&'a [u8]>,
    sha256sums: Vec<&'a [u8]>,
    sha384sums: Vec<&'a [u8]>,
    sha512sums: Vec<&'a [u8]>,
    b2sums: Vec<&'a [u8]>,
    pkgver_func: bool,
}

impl<'a> Default for PkgbuildParsing<'a> {
    fn default() -> Self {
        Self {
            base: b"",
            pkgs: vec![],
            deps: vec![],
            makedeps: vec![],
            provides: vec![],
            sources: vec![],
            cksums: vec![],
            md5sums: vec![],
            sha1sums: vec![],
            sha224sums: vec![],
            sha256sums: vec![],
            sha384sums: vec![],
            sha512sums: vec![],
            b2sums: vec![],
            pkgver_func: false,
        }
    }
}

struct PkgbuildsParsing<'a> {
    entries: Vec<PkgbuildParsing<'a>>
}

impl<'a> PkgbuildsParsing<'a> {
    fn from_parser_output(output: &'a Vec<u8>) -> Result<Self> {
        let mut pkgbuilds = Vec::new();
        let mut pkgbuild = PkgbuildParsing::default();
        let mut started = false;
        for line in output.split(|byte| *byte == b'\n') {
            if line.is_empty() { continue }
            if line.contains(&b':') {
                let mut it =
                    line.splitn(2, |byte| byte == &b':');
                let key = match it.next() {
                    Some(key) => key,
                    None => {
                        log::error!("Failed to get key from PKGBUILD, at line: \
                            '{}'", String::from_utf8_lossy(line));
                        return Err(Error::BrokenPKGBUILDs(Vec::new()));
                    },
                };
                let value = match it.next() {
                    Some(value) => value,
                    None => {
                        log::error!("Failed to get value from PKGBUILD, at line\
                            : '{}'", String::from_utf8_lossy(line));
                        return Err(Error::BrokenPKGBUILDs(Vec::new()));
                    },
                };
                if it.next().is_some() {
                    log::error!("PKGBUILD parsing line not sustained: '{}'", 
                        String::from_utf8_lossy(line));
                    return Err(Error::ParserScriptIllegalOutput(line.into()));
                }
                match key {
                    b"base" => pkgbuild.base = value,
                    b"name" => {
                        let mut pkg =
                            PackageParsing::default();
                        pkg.name = value;
                        pkgbuild.pkgs.push(pkg);
                    },
                    b"dep" => pkgbuild.deps.push(value),
                    b"makedep" => pkgbuild.makedeps.push(value),
                    b"provide" => pkgbuild.provides.push(value),
                    b"source" => pkgbuild.sources.push(value),
                    b"ck" => pkgbuild.cksums.push(value),
                    b"md5" => pkgbuild.md5sums.push(value),
                    b"sha1" => pkgbuild.sha1sums.push(value),
                    b"sha224" => pkgbuild.sha224sums.push(value),
                    b"sha256" => pkgbuild.sha256sums.push(value),
                    b"sha384" => pkgbuild.sha384sums.push(value),
                    b"sha512" => pkgbuild.sha512sums.push(value),
                    b"b2" => pkgbuild.b2sums.push(value),
                    b"pkgver_func" => match value {
                        b"y" => pkgbuild.pkgver_func = true,
                        b"n" => pkgbuild.pkgver_func = false,
                        _ => {
                            log::error!("Unexpected value: {}",
                                String::from_utf8_lossy(value));
                            return Err(Error::BrokenPKGBUILDs(Vec::new()))
                        }
                    }
                    _ => {
                        let (offset, is_dep) =
                        if key.starts_with(b"dep_") {(4, true)}
                        else if key.starts_with(b"provide_") {(8, false)}
                        else {
                            log::error!("Unexpected line: {}",
                                String::from_utf8_lossy(line));
                            return Err(Error::BrokenPKGBUILDs(Vec::new()))
                        };
                        let name = &key[offset..];
                        let mut pkg = None;
                        for pkg_cmp in
                            pkgbuild.pkgs.iter_mut()
                        {
                            if pkg_cmp.name == name {
                                pkg = Some(pkg_cmp);
                                break
                            }
                        }
                        let pkg = match pkg {
                            Some(pkg) => pkg,
                            None => {
                                log::error!("Failed to find pkg {}",
                                    String::from_utf8_lossy(name));
                                return Err(Error::BrokenPKGBUILDs(Vec::new()))
                            },
                        };
                        if is_dep {
                            pkg.deps.push(value)
                        } else {
                            pkg.provides.push(value)
                        }
                    }
                }
            } else if line == b"[PKGBUILD]" {
                if started {
                    pkgbuilds.push(pkgbuild);
                    pkgbuild = PkgbuildParsing::default();
                } else {
                    started = true
                }
            } else {
                log::error!("Illegal line: {}", String::from_utf8_lossy(line));
                return Err(Error::BrokenPKGBUILDs(Vec::new()))
            }
        }
        pkgbuilds.push(pkgbuild);
        Ok(Self {
            entries: pkgbuilds,
        })
    }
}

/// A sub-package parsed from a split-package `PKGBUILD`, owned variant
/// during parsing. Library users should not used this.
#[derive(Debug)]
pub struct Package {
    pub name: String,
    pub deps: Vec<String>,
    pub provides: Vec<String>,
}

#[derive(Debug)]
pub struct Pkgbuild {
    pub base: String,
    pub pkgs: Vec<Package>,
    pub deps: Vec<String>,
    pub makedeps: Vec<String>,
    pub provides: Vec<String>,
    pub sources: Vec<String>,
    pub cksums: Vec<String>,
    pub md5sums: Vec<String>,
    pub sha1sums: Vec<String>,
    pub sha224sums: Vec<String>,
    pub sha256sums: Vec<String>,
    pub sha384sums: Vec<String>,
    pub sha512sums: Vec<String>,
    pub b2sums: Vec<String>,
    pub pkgver_func: bool,
}
pub(crate) struct Pkgbuilds {
    entries: Vec<Pkgbuild>
}

fn vec_string_from_vec_u8(original: &Vec<&[u8]>) -> Vec<String> {
    original.iter().map(|item|
        String::from_utf8_lossy(item).into_owned()).collect()
}

impl Package {
    fn from_borrowed(borrowed: &PackageParsing) -> Self {
        Self {
            name: String::from_utf8_lossy(borrowed.name).into_owned(),
            deps: vec_string_from_vec_u8(&borrowed.deps),
            provides: vec_string_from_vec_u8(&borrowed.provides),
        }
    }
}

impl Pkgbuild {
    fn from_borrowed(borrowed: &PkgbuildParsing) -> Self {
        Self {
            base: String::from_utf8_lossy(borrowed.base).into_owned(),
            pkgs: borrowed.pkgs.iter().map(|pkg|
                Package::from_borrowed(pkg)).collect(),
            deps: vec_string_from_vec_u8(&borrowed.deps),
            makedeps: vec_string_from_vec_u8(&borrowed.makedeps),
            provides: vec_string_from_vec_u8(&borrowed.provides),
            sources: vec_string_from_vec_u8(&borrowed.sources),
            cksums: vec_string_from_vec_u8(&borrowed.cksums),
            md5sums: vec_string_from_vec_u8(&borrowed.md5sums),
            sha1sums: vec_string_from_vec_u8(&borrowed.sha1sums),
            sha224sums: vec_string_from_vec_u8(&borrowed.sha224sums),
            sha256sums: vec_string_from_vec_u8(&borrowed.sha256sums),
            sha384sums: vec_string_from_vec_u8(&borrowed.sha384sums),
            sha512sums: vec_string_from_vec_u8(&borrowed.sha512sums),
            b2sums: vec_string_from_vec_u8(&borrowed.b2sums),
            pkgver_func: borrowed.pkgver_func,
        }
    }
}

impl Pkgbuilds {
    fn from_borrowed(borrowed: PkgbuildsParsing) -> Self {
        Self {
            entries: borrowed.entries.iter().map(
                |entry|
                    Pkgbuild::from_borrowed(entry)).collect(),
        }
    }
}