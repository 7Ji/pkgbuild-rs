use std::{collections::BTreeMap, ffi::{OsStr, OsString}, fmt::{Display, Formatter}, io::{BufWriter, Read, Write}, os::unix::ffi::OsStrExt, path::{Path, PathBuf}, process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio}};

use hex::FromHex;
#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};
#[cfg(feature = "nothread")]
use nix::fcntl::{fcntl, FcntlArg::F_SETFL, OFlag};
#[cfg(feature = "nothread")]
use std::os::fd::AsRawFd;
#[cfg(not(feature = "nothread"))]
use std::thread::spawn;
#[cfg(feature = "vercmp")]
use std::cmp::Ordering;

#[cfg(feature = "unsafe_str")]
macro_rules! str_from_slice_u8 {
    ($l:expr) => {unsafe{std::str::from_utf8_unchecked($l)}}
}

#[cfg(not(feature = "unsafe_str"))]
macro_rules! str_from_slice_u8 {
    ($l:expr) => {String::from_utf8_lossy($l).as_ref()}
}

#[cfg(feature = "unsafe_str")]
macro_rules! string_from_slice_u8 {
    ($l:expr) => {unsafe{String::from_utf8_unchecked($l.into())}}
}

#[cfg(not(feature = "unsafe_str"))]
macro_rules! string_from_slice_u8 {
    ($l:expr) => {String::from_utf8_lossy($l).to_string()}
}

#[cfg(feature = "serde")]
mod serde_optional_bytes_arrays {
    use serde::de::{Error, Visitor};
    use serde::ser::SerializeSeq;
    use serde::{Serializer, Deserializer};

    use serde_bytes::{Bytes, ByteBuf};

    pub(crate) fn serialize<const N: usize, S>(
        bytes_arrays: &Vec<Option<[u8; N]>>, serializer: S
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq_arrays = 
            serializer.serialize_seq(Some(bytes_arrays.len()))?;
        for byte_array in bytes_arrays.iter() {
            let wrapped = match byte_array {
                Some(byte_array) => Some(Bytes::new(byte_array)),
                None => None,
            };
            seq_arrays.serialize_element(&wrapped)?;
        }
        seq_arrays.end()
    }

    struct VecVisitor;


    impl<'de> Visitor<'de> for VecVisitor {
        type Value = Vec<Option<ByteBuf>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) 
        -> std::fmt::Result 
        {
            formatter.write_str("data is not optional byte arrays")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>, 
        {
            let mut arrays = Vec::new();
            loop {
                let next: Result<Option<Option<ByteBuf>>, _> 
                    = seq.next_element();
                match next {
                    Ok(array) => 
                        if let Some(array) = array {
                            arrays.push(array)
                        } else {
                            break
                        },
                    Err(e) => return Err(e),
                }
            }
            Ok(arrays)
        }

    }

    pub(crate) fn deserialize<'de, D, const N: usize>(deserializer: D) 
    -> Result<Vec<Option<[u8; N]>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let arrays_generic 
            = deserializer.deserialize_seq(VecVisitor)?;
        let mut arrays_typed = Vec::new();
        for array in arrays_generic {
            if let Some(array) = array {
                let array_typed = ((&array) as &[u8]).try_into().map_err(
                |_| {
                    let expected = format!("[u8; {}]", N);
                    D::Error::invalid_length(array.len(), &expected.as_str())
                })?;
                arrays_typed.push(Some(array_typed))
            } else {
                arrays_typed.push(None)
            }
        }
        Ok(arrays_typed)
    }
}

#[derive(Debug)]
pub enum Error {
    /// Some I/O error happended, possibly during the script generation
    IoError(std::io::Error),
    /// Nix Errno, possibly returned when we try to set child stdin/out/err
    /// as non-blocking
    #[cfg(feature = "nothread")]
    NixErrno(nix::errno::Errno),
    /// The parsed result count is different from our input, but it might still
    /// be usable
    MismatchedResultCount {
        input: usize,
        output: usize,
        result: Vec<Pkgbuild>
    },
    /// The child's Stdio handles are incomplete and we can't get, this is not
    /// fixable, but intentionally not panic to reduce damage to caller
    ChildStdioIncomplete,
    /// The child has bad return
    ChildBadReturn(Option<i32>),
    /// Some thread paniked and not joinable, this should not happen in our 
    /// code explicitly
    #[cfg(not(feature = "nothread"))]
    ThreadUnjoinable,
    /// Some PKGBUILDs were broken, this contains a list of those PKGBUILDs
    BrokenPKGBUILDs(Vec<String>),
    /// The parser script has returned some unexpected, illegal output
    ParserScriptIllegalOutput(Vec<u8>)
}

impl Clone for Error {
    fn clone(&self) -> Self {
        match self {
            Self::IoError(arg0) => Self::IoError(arg0.kind().into()),
            #[cfg(feature = "nothread")]
            Self::NixErrno(arg0) => Self::NixErrno(*arg0),
            Self::MismatchedResultCount { 
                input, output, result 
            } => Self::MismatchedResultCount { 
                    input: input.clone(), 
                    output: output.clone(), 
                    result: result.clone() },
            Self::ChildStdioIncomplete => Self::ChildStdioIncomplete,
            Self::ChildBadReturn(arg0) => 
                Self::ChildBadReturn(arg0.clone()),
            #[cfg(not(feature = "nothread"))]
            Self::ThreadUnjoinable => Self::ThreadUnjoinable,
            Self::BrokenPKGBUILDs(arg0) => 
                Self::BrokenPKGBUILDs(arg0.clone()),
            Self::ParserScriptIllegalOutput(arg0) => 
                Self::ParserScriptIllegalOutput(arg0.clone()),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::IoError(value)
    }
}

#[cfg(feature = "nothread")]
impl From<nix::errno::Errno> for Error {
    fn from(value: nix::errno::Errno) -> Self {
        Self::NixErrno(value)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::IoError(e) => write!(f, "IO Error: {}", e),
            #[cfg(feature = "nothread")]
            Error::NixErrno(e) => write!(f, "Nix Errno: {}", e),
            Error::MismatchedResultCount { 
                input, output, result: _ 
            } => write!(f, "Result Count Mismatch: Input {}, Output {}",
                    input, output),
            Error::ChildStdioIncomplete => write!(f, "Child StdIO incomplete"),
            Error::ChildBadReturn(e) => 
                write!(f, "Child Bad Return: {:?}", e),
            #[cfg(not(feature = "nothread"))]
            Error::ThreadUnjoinable => write!(f, "Thread Not Joinable"),
            Error::BrokenPKGBUILDs(e) => 
                write!(f, "PKGBUILDs Broken ({})", e.len()),
            Error::ParserScriptIllegalOutput(e) => write!(
                f, "Parser Script Illegal Output: {}", str_from_slice_u8!(e)),
        }
    }
}

impl std::error::Error for Error {}

/// The script builder to construct a `ParserScript` dynamically
pub struct ParserScriptBuilder {
    /// The path to makepkg library, usually `/usr/share/makepkg` on an Arch 
    /// installation
    pub makepkg_library: OsString,

    /// The makepkg configuration file, usually `/etc/makepkg.conf` on an Arch
    /// installation
    pub makepkg_config: OsString,

    /// Should we dump `pkgbase` from `PKGBUILD`, if disabled then `pkgbase` in
    /// the parsed `Pkgbuild` struct would be empty.
    /// 
    /// Disabling this should reduce a few micro seconds of parsing time for
    /// each `PKGBUILD`, but note the underlying value would still be read and
    /// initialized into their Bash variable anyway.
    /// 
    /// Default: `true`
    pub pkgbase: bool,

    /// Should we dump `pkgname` from `PKGBUILD`, if disabled then `pkgs` in the
    /// parsed `Pkgbuild` struct would be empty, as if they're not split-package
    /// `PKGBUILD`s.
    /// 
    /// Disabling this should reduce a few micro seconds of parsing time for
    /// each `PKGBUILD`, but note the underlying value would still be read and
    /// initialized into their Bash array anyway.
    /// 
    /// Default: `true`
    pub pkgname: bool,


    /// Should the parser dump `pkgver` from `PKGBUILD`
    /// 
    /// Default: `true`
    pub pkgver: bool,

    /// Should the parser dump `pkgrel` from `PKGBUILD`
    /// 
    /// Default: `true`
    pub pkgrel: bool,

    /// Should the parser dump `epoch` from `PKGBUILD`
    /// 
    /// Default: `true`
    pub epoch: bool,

    /// Should the parser dump `arch` from `PKGBUILD`
    /// 
    /// Default: `true`
    pub arch: bool,

    /// Should the `depends` array be dumped. If not, then `depends` array in
    /// the parsed `Pkgbuild` struct would be empty. 
    /// 
    /// Disabling this should reduce a few micro seconds of parsing time for
    /// each `PKGBUILD`, but note the underlying array would still be read and
    /// initialized into their Bash array anyway.
    /// 
    /// Default: `true`
    pub depends: bool,

    /// Should the `makedepends` array be dumped. If not, then `makedepends` 
    /// array in the parsed `Pkgbuild` struct would be empty. 
    /// 
    /// Disabling this should reduce a few micro seconds of parsing time for
    /// each `PKGBUILD`, but note the underlying array would still be read and
    /// initialized into their Bash array anyway.
    /// 
    /// Default: `true`
    pub makedepends: bool,

    /// Should the `makedepends` array be dumped. If not, then `makedepends` 
    /// array in the parsed `Pkgbuild` struct would be empty. 
    /// 
    /// Disabling this should reduce a few micro seconds of parsing time for
    /// each `PKGBUILD`, but note the underlying array would still be read and
    /// initialized into their Bash array anyway.
    /// 
    /// Default: `true`
    pub provides: bool,

    /// Should the `source` array be dumped. If not, then `source` array in the 
    /// parsed `Pkgbuild` struct would be empty. 
    /// 
    /// Disabling this should reduce a few micro seconds of parsing time for
    /// each `PKGBUILD`, but note the underlying array would still be read and
    /// initialized into their Bash array anyway.
    /// 
    /// Default: `true`
    pub source: bool,

    /// Should the `cksums` array be dumped.
    /// 
    /// Default: `true`
    pub cksums: bool,

    /// Should the `md5sums` array be dumped.
    /// 
    /// Default: `true`
    pub md5sums: bool,

    /// Should the `sha1sums` array be dumped.
    /// 
    /// Default: `true`
    pub sha1sums: bool,

    /// Should the `sha224sums` array be dumped.
    /// 
    /// Default: `true`
    pub sha224sums: bool,

    /// Should the `sha256sums` array be dumped.
    /// 
    /// Default: `true`
    pub sha256sums: bool,

    /// Should the `sha384sums` array be dumped.
    /// 
    /// Default: `true`
    pub sha384sums: bool,

    /// Should the `sha512sums` array be dumped.
    /// 
    /// Default: `true`
    pub sha512sums: bool,

    /// Should the `cksums` array be dumped.
    /// 
    /// Default: `true`
    pub b2sums: bool,

    /// Should the parser detect if a `pkgver()` function exists for the parsed
    /// `PKGBUILD`
    /// 
    /// Disabling this should reduce a few micro seconds of parsing time.
    /// 
    /// Default: `true`
    pub pkgver_func: bool,

    /// Whether the parser should expand the `package()` and all of the 
    /// `package_${pkgname}()` functions to extract the package-specific 
    /// `depends`
    /// 
    /// If enabled the functions would need to be dumped internally in the 
    /// `Bash` world into newline-sepeareted arrays, and simple line-beginning
    /// match would need to be performed on the lines to find `depends=(` and 
    /// `eval` on the line. Even though this is all done purely in `Bash` the
    /// method does introduce tens of microseconds of parsing time for each 
    /// `PKGBUILD`.
    /// 
    /// _(Note: this is done differently from `makepkg` library, which uses
    /// external text-parsing utils for the job, and that is a couple times 
    /// slower due to program starting and ripping)_
    /// 
    /// Note that, if `pkgname` is disabled, only the non-split-package 
    /// `package()` would be expanded.
    /// 
    /// Default: `true`
    pub package_depends: bool,

    /// Whether the parser should expand the `package()` and all of the 
    /// `package_${pkgname}()` functions to extract the package-specific 
    /// `makedepends`
    /// 
    /// If enabled the functions would need to be dumped internally in the 
    /// `Bash` world into newline-sepeareted arrays, and simple line-beginning
    /// match would need to be performed on the lines to find `makedepends=(` 
    /// and `eval` on the line. Even though this is all done purely in `Bash`
    /// the method does introduce tens of microseconds of parsing time for each 
    /// `PKGBUILD`.
    /// 
    /// _(Note: this is done differently from `makepkg` library, which uses
    /// external text-parsing utils for the job, and that is a couple times 
    /// slower due to program starting and ripping)_
    /// 
    /// Note that, if `pkgname` is disabled, only the non-split-package 
    /// `package()` would be expanded.
    /// 
    /// Default: `true`
    pub package_makedepends: bool,
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
            makepkg_config: env_or("MAKEPKG_CONF", "/etc/makepkg.conf"),
            pkgbase: true,
            pkgname: true,
            pkgver: true,
            pkgrel: true,
            epoch: true,
            arch: true,
            depends: true,
            makedepends: true,
            provides: true,
            source: true,
            cksums: true,
            md5sums: true,
            sha1sums: true,
            sha224sums: true,
            sha256sums: true,
            sha384sums: true,
            sha512sums: true,
            b2sums: true,
            pkgver_func: true,
            package_depends: true,
            package_makedepends: true, 
        }
    }
}

macro_rules! set_bool {
    ($item:ident, $func:ident, $var:ident) => {
        pub fn $func(&mut $item, value: bool) -> &mut Self {
            $item.$var = value;
            $item
        }
    };
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

    set_bool!(self, set_pkgbase, pkgbase);
    set_bool!(self, set_pkgname, pkgname);
    set_bool!(self, set_pkgver, pkgver);
    set_bool!(self, set_pkgrel, pkgrel);
    set_bool!(self, set_epoch, epoch);
    set_bool!(self, set_arch, arch);
    set_bool!(self, set_depends, depends);
    set_bool!(self, set_makedepends, makedepends);
    set_bool!(self, set_provides, provides);
    set_bool!(self, set_cksums, cksums);
    set_bool!(self, set_md5sums, md5sums);
    set_bool!(self, set_sha1sums, sha1sums);
    set_bool!(self, set_sha224sums, sha224sums);
    set_bool!(self, set_sha256sums, sha256sums);
    set_bool!(self, set_sha384sums, sha384sums);
    set_bool!(self, set_sha512sums, sha512sums);
    set_bool!(self, set_b2sums, b2sums);
    set_bool!(self, set_pkgver_func, pkgver_func);
    set_bool!(self, set_package_depends, package_depends);
    set_bool!(self, set_package_makedepends, package_makedepends);

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
        writer.write_all(include_bytes!("script/full.bash"))
    //     writer.write_all(include_bytes!(
    //         "script/10_source_lib_config.bash"))?;
    //     let func_dump_array: &[u8] = 
    //         if self.arch {
    //             writer.write_all(include_bytes!(
    //                 "script/21_func_dump_array_with_optional_arch.bash"))?;
    //             b"dump_array_with_optional_arch"
    //         } else {
    //             writer.write_all(include_bytes!(
    //                 "script/20_func_dump_array.bash"))?;
    //             b"dump_array"
    //         };
    //     if self.package_depends || self.package_makedepends {
    //         writer.write_all(include_bytes!(
    //             "script/22_func_extract_package_vars.bash"))?;
    //     }
    //     writer.write_all(include_bytes!("script/30_loop_start.bash"))?;
    //     if self.pkgbase {
    //         writer.write_all(b"echo \"base:${pkgbase}\"\n")?;
    //     }
    //     if self.pkgname {
    //         writer.write_all(b"for item in \"${pkgname[@]}\"; do \
    //                                 echo \"name:${item}\"; done\n")?
    //     } else {
    //         writer.write_all(b"for item in \"${pkgname[@]}\"; do \
    //                                     unset -f package_\"${item}\"; \
    //                                 done\n\
    //                                 pkgname=\"${pkgbase}\"\n")?
    //     }
    //     if self.pkgver {
    //         writer.write_all(b"echo \"ver:${pkgver}\"\n")?
    //     }
    //     if self.pkgrel {
    //         writer.write_all(b"echo \"rel:${pkgrel}\"\n")?
    //     }
    //     if self.epoch {
    //         writer.write_all(b"echo \"epoch:${epoch}\"\n")?
    //     }
    //     if self.depends {
    //         writer.write_all(func_dump_array)?;
    //         writer.write_all(b" depends dep\n")?
    //     }
    //     if self.makedepends {
    //         writer.write_all(func_dump_array)?;
    //         writer.write_all(b" makedepends makedep\n")?
    //     }
    //     if self.provides {
    //         writer.write_all(func_dump_array)?;
    //         writer.write_all(b" provides provide\n")?
    //     }
    //     if self.source {
    //         writer.write_all(func_dump_array)?;
    //         writer.write_all(b" source source\n")?
    //     }
    //     if self.cksums {
    //         writer.write_all(func_dump_array)?;
    //         writer.write_all(b" cksums ck\n")?
    //     }
    //     if self.md5sums {
    //         writer.write_all(func_dump_array)?;
    //         writer.write_all(b" md5sums md5\n")?
    //     }
    //     if self.sha1sums {
    //         writer.write_all(func_dump_array)?;
    //         writer.write_all(b" sha1sums sha1\n")?
    //     }
    //     if self.sha224sums {
    //         writer.write_all(func_dump_array)?;
    //         writer.write_all(b" sha224sums sha224\n")?
    //     }
    //     if self.sha256sums {
    //         writer.write_all(func_dump_array)?;
    //         writer.write_all(b" sha256sums sha256\n")?
    //     }
    //     if self.sha384sums {
    //         writer.write_all(func_dump_array)?;
    //         writer.write_all(b" sha384sums sha384\n")?
    //     }
    //     if self.sha512sums {
    //         writer.write_all(func_dump_array)?;
    //         writer.write_all(b" sha512sums sha512\n")?
    //     }
    //     if self.b2sums {
    //         writer.write_all(func_dump_array)?;
    //         writer.write_all(b" b2sums b2\n")?
    //     }
    //     if self.pkgver_func {
    //         writer.write_all(b"echo -n 'pkgver_func:'\n")?;
    //         writer.write_all(b"[[ $(type -t pkgver) == 'function' ]] && \
    //                             echo y || echo n\n")?
    //     }
    //     if self.pkgname && (self.package_depends || self.package_makedepends) {
    //         writer.write_all(
    //             b"unset -v {depends,provides}{,_\"${CARCH}\"}\n\
    //             extract_package_vars\n")?;
    //         if self.package_depends {
    //             writer.write_all(func_dump_array)?;
    //             writer.write_all(b" depends dep_\"${pkgbase}\"\n")?
    //         }
    //         if self.package_makedepends {
    //             writer.write_all(func_dump_array)?;
    //             writer.write_all(b" provides provide_\"${pkgbase}\"\n")?
    //         }
    //         writer.write_all(b"for item in \"${pkgname[@]}\"; do\n\
    //             unset -v {depends,provides}{,_\"${CARCH}\"}\n\
    //             extract_package_vars _\"${item}\"\n")?;
    //         if self.package_depends {
    //             writer.write_all(func_dump_array)?;
    //             writer.write_all(b" depends dep_\"${item}\"\n")?
    //         }
    //         if self.package_makedepends {
    //             writer.write_all(func_dump_array)?;
    //             writer.write_all(b" provides provide_\"${item}\"\n")?
    //         }
    //         writer.write_all(b"done\n")?
    //     }
    //     writer.write_all(b"unset -v pkgbase pkgname pkgver pkgrel epoch \
    //         {depends,makedepends,provides,source,\
    //         {ck,md5,sha{1,224,256,384,512},b2}sums}{,_\"${CARCH}\"}\n\
    //         unset -f pkgver package\n\
    //         done\n")
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
    #[cfg(feature = "tempfile")]
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

    /// Build a `ParserScript`, at given path, which would could later be used 
    /// to parse `PKGBUILD`s
    /// 
    /// Return `Ok(ParserScript)` if write was successfull, return `Err` on IO
    /// Error.
    #[cfg(not(feature = "tempfile"))]
    pub fn build<P: AsRef<Path>>(&self, path: P) -> Result<ParserScript> {
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
    }
}

pub enum ParserScript {
    #[cfg(feature = "tempfile")]
    Temporary(tempfile::NamedTempFile),
    Persistent(PathBuf),
}

impl AsRef<OsStr> for ParserScript {
    fn as_ref(&self) -> &OsStr {
        match self {
            #[cfg(feature = "tempfile")]
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
    #[cfg(feature = "tempfile")]
    pub fn new<P: AsRef<Path>>(path: Option<P>) -> Result<Self> {
        ParserScriptBuilder::new().build(path)
    }

    /// Generate a parser script at the given path
    /// 
    /// This uses either `LIBRARY` from env or `/usr/share/makekg` if the env
    /// is missing for `makepkg_library` (named `LIBRARY` in  `makepkg` 
    /// routines) and either `MAKEPKG_CONF` from env or `/etc/makepkg.conf` if
    /// the env is missing for `makepkg_config` (named `MAKEPKG_CONF` in 
    /// `makepkg` routines). 
    /// 
    /// If customization on those variables are needed, then caller should 
    /// create a `ParserScript` with a `ParserScriptBuilder`
    #[cfg(not(feature = "tempfile"))]
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
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
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            intepreter: "/bin/bash".into(),
            work_dir: None
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

#[cfg(feature = "nothread")]
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
    #[cfg(feature = "nothread")]
    fn set_nonblock(&mut self) -> Result<()> {   
        set_nonblock(&self.stdin)?;
        set_nonblock(&self.stdout)?;
        set_nonblock(&self.stderr)
    }

    /// This is a sub-optimal single-thread implementation, extra times would
    /// be wasted on inefficient page-by-page try-reading to avoid jamming the
    /// child stdin/out/err.
    #[cfg(feature = "nothread")]
    fn work(mut self, input: &[u8]) -> Result<(Vec<u8>, Vec<u8>)>{
        use libc::{PIPE_BUF, EAGAIN};

        self.set_nonblock()?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut buffer = vec![0; PIPE_BUF];
        let buffer = buffer.as_mut_slice();
        let mut written = 0;
        let total = input.len();
        let mut stdout_finish = false;
        let mut stderr_finish = false;
        // Rotate among stdin, stdout and stderr to avoid jamming
        loop {
            // Try to write at most the length of a PIPE buffer
            let mut end = written + PIPE_BUF;
            if end > total {
                end = total;
            }
            match self.stdin.write(&input[written..end]) {
                Ok(written_this) => {
                    written += written_this;
                    if written >= total {
                        drop(self.stdin);
                        break
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
            if ! stdout_finish {
                match self.stdout.read (&mut buffer[..]) {
                    Ok(read_this) =>
                        if read_this > 0 {
                            stdout.extend_from_slice(&buffer[0..read_this])
                        } else {
                            stdout_finish = true;
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
        }
        // Rotate between stdout and stderr to avoid jamming
        loop {
            if ! stdout_finish {
                match self.stdout.read (&mut buffer[..]) {
                    Ok(read_this) =>
                        if read_this > 0 {
                            stdout.extend_from_slice(&buffer[0..read_this])
                        } else {
                            stdout_finish = true;
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
            if stdout_finish && stderr_finish {
                break
            }
        }
        drop(self.stdout);
        drop(self.stderr);
        Ok((stdout, stderr))
    }

    /// The multi-threaded 
    #[cfg(not(feature = "nothread"))]
    fn work(mut self, mut input: Vec<u8>) 
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
    #[cfg(feature = "tempfile")]
    pub fn new() -> Result<Self> {
        let script = ParserScript::new(None::<&str>)?;
        let options = ParserOptions::default();
        Ok(Self{
            script,
            options,
        })
    }

    /// Create a new parser with default settings, with parser script created
    /// at the given path
    #[cfg(not(feature = "tempfile"))]
    pub fn new<P: AsRef<Path>>(script_path: P) -> Result<Self> {
        let script = ParserScript::new(script_path)?;
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
            // .arg("-e")
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
            let line = path.as_ref().as_os_str().as_bytes();
            if ! line.contains(&b'/') {
                input.extend_from_slice(b"./");
            }
            input.extend_from_slice(line);
            input.push(b'\n')
        }
        if count == 0 {
            return Ok(Vec::new())
        }
        let (mut child, child_ios) = self.get_child_taken()?;
        // Do not handle the error yet, wait for the child to finish first
        #[cfg(not(feature = "nothread"))]
        let out_and_err = child_ios.work(input);
        #[cfg(feature = "nothread")]
        let out_and_err = child_ios.work(&input);
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
                    return Err(Error::ChildBadReturn(status.code()))
                }
                (out, err)
            },
            Err(e) => {
                if let Err(e) = child.kill() {
                    log::error!("Failed to kill child after failed parsing");
                    return Err(e.into())
                }
                match child.wait() {
                    Ok(status) =>
                        log::warn!("Killed child return: {}", status),
                    Err(e) => {
                        log::error!("Failed to wait for killed child: {}", e);
                        return Err(e.into())
                    }
                }
                return Err(e)
            },
        };
        if ! err.is_empty() {
            log::warn!("Parser has written to stderr: \n{}", 
                str_from_slice_u8!(&err));
        }
        if log::log_enabled!(log::Level::Debug) {
            log::debug!("Raw output from parser:\n{}", 
                str_from_slice_u8!(&out));
        }
        let pkgbuilds = Pkgbuilds::try_from(
            &PkgbuildsParsing::from_parser_output(&out)?)?;
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
#[cfg(feature = "tempfile")]
pub fn parse_multi<I, P>(paths: I) -> Result<Vec<Pkgbuild>>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>
{
    Parser::new()?.parse_multi(paths)
}

/// A shortcut to create a `Parser` and parse multiple `PKGBUILD`s, with the
/// parser script created at the given path
#[cfg(not(feature = "tempfile"))]
pub fn parse_multi<I, P1, P2>(script_path: P1, pkgbuild_paths: I) 
-> Result<Vec<Pkgbuild>>
where
    I: IntoIterator<Item = P2>,
    P1: AsRef<Path>,
    P2: AsRef<Path>
{
    Parser::new(script_path)?.parse_multi(pkgbuild_paths)
}

/// A shortcut to create a `Parser` and parse a single `PKGBUILD`
#[cfg(feature = "tempfile")]
pub fn parse_one<P>(path: Option<P>) -> Result<Pkgbuild>
where
    P: AsRef<Path> 
{
    Parser::new()?.parse_one(path)
}

/// A shortcut to create a `Parser` and parse a single `PKGBUILD`, with the
/// parser script created at the given path
#[cfg(not(feature = "tempfile"))]
pub fn parse_one<P1, P2>(script_path: P1, pkgbuild_path: Option<P2>) 
-> Result<Pkgbuild>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>
{
    Parser::new(script_path)?.parse_one(pkgbuild_path)
}

#[derive(Default, Debug)]
struct PackageArchitectureParsing<'a> {
    arch: &'a [u8],
    checkdepends: Vec<&'a [u8]>,
    depends: Vec<&'a [u8]>,
    optdepends: Vec<&'a [u8]>,
    provides: Vec<&'a [u8]>,
    conflicts: Vec<&'a [u8]>,
    replaces: Vec<&'a [u8]>,
}

/// A sub-package parsed from a split-package `PKGBUILD`, borrowed variant
/// during parsing. Library users should not used this.
#[derive(Default, Debug)]
struct PackageParsing<'a> {
    pkgname: &'a [u8],
    pkgdesc: &'a [u8],
    url: &'a [u8],
    license: Vec<&'a [u8]>,
    groups: Vec<&'a [u8]>,
    backup: Vec<&'a [u8]>,
    options: Vec<&'a [u8]>,
    install: &'a [u8],
    changelog: &'a [u8],
    arches: Vec<PackageArchitectureParsing<'a>>,
}

#[derive(Default, Debug)]
struct PkgbuildArchitectureParsing<'a> {
    arch: &'a [u8],
    sources: Vec<&'a [u8]>,
    cksums: Vec<&'a [u8]>,
    md5sums: Vec<&'a [u8]>,
    sha1sums: Vec<&'a [u8]>,
    sha224sums: Vec<&'a [u8]>,
    sha256sums: Vec<&'a [u8]>,
    sha384sums: Vec<&'a [u8]>,
    sha512sums: Vec<&'a [u8]>,
    b2sums: Vec<&'a [u8]>,
    depends: Vec<&'a [u8]>,
    makedepends: Vec<&'a [u8]>,
    checkdepends: Vec<&'a [u8]>,
    optdepends: Vec<&'a [u8]>,
    conflicts: Vec<&'a [u8]>,
    provides: Vec<&'a [u8]>,
    replaces: Vec<&'a [u8]>,
}

/// A `PKGBUILD` being parsed. Library users should
/// not use this.
#[derive(Default, Debug)]
struct PkgbuildParsing<'a> {
    pkgbase: &'a [u8],
    pkgs: Vec<PackageParsing<'a>>,
    pkgver: &'a [u8],
    pkgrel: &'a [u8],
    epoch: &'a [u8],
    pkgdesc: &'a [u8],
    url: &'a [u8],
    license: Vec<&'a [u8]>,
    install: &'a [u8],
    changelog: &'a [u8],
    validgpgkeys: Vec<&'a [u8]>,
    noextract: Vec<&'a [u8]>,
    groups: Vec<&'a [u8]>,
    arches: Vec<PkgbuildArchitectureParsing<'a>>,
    backups: Vec<&'a [u8]>,
    options: Vec<&'a [u8]>,
    pkgver_func: bool,
}

#[derive(Default, Debug)]
struct PkgbuildsParsing<'a> {
    entries: Vec<PkgbuildParsing<'a>>
}

#[derive(Debug)]
enum ParsingState<'a> {
    None,
    Pkgbuild (PkgbuildParsing<'a>),
    Package (PkgbuildParsing<'a>, PackageParsing<'a>),
    PackageArchSpecific (PkgbuildParsing<'a>, 
        PackageParsing<'a>, PackageArchitectureParsing<'a>),
    PkgbuildArchSpecific (PkgbuildParsing<'a>, PkgbuildArchitectureParsing<'a>),
}

impl<'a> PkgbuildsParsing<'a> {
    fn from_parser_output(output: &'a Vec<u8>) -> Result<Self> {
        let mut pkgbuilds = Vec::new();
        let mut state = ParsingState::None;
        for line in output.split(|byte| *byte == b'\n') {
            macro_rules! key_value_from_slice_u8 {
                ($slice:ident, $key:ident, $value: ident) => {
                    let mut it = $slice.splitn(2, |byte|*byte == b':');
                    let $key = it.next().unwrap_or_default();
                    let $value = it.next().unwrap_or_default();
                };
            }
            if line.is_empty() { continue }
            match state {
                ParsingState::None => 
                match line {
                    b"PKGBUILD" => state = ParsingState::Pkgbuild(
                                        PkgbuildParsing::default()),
                    _ => {
                        log::error!("Line '{}' encountered when expecting \
                            [PKGBUILD]", str_from_slice_u8!(line));
                        return Err(Error::ParserScriptIllegalOutput(line.into()))
                    }
                },
                ParsingState::Pkgbuild(mut pkgbuild) => {
                match line {
                    b"PACKAGE" => state = 
                        ParsingState::Package(pkgbuild, Default::default()),
                    b"ARCH" => state = ParsingState::PkgbuildArchSpecific(
                                        pkgbuild, Default::default()),
                    b"END" => {
                        pkgbuilds.push(pkgbuild);
                        state = ParsingState::None
                    },
                    _ => {
                        key_value_from_slice_u8!(line, key, value);
                        match key {
                            b"pkgbase" => pkgbuild.pkgbase = value,
                            b"pkgver" => pkgbuild.pkgver = value,
                            b"pkgrel" => pkgbuild.pkgrel = value,
                            b"epoch" => pkgbuild.epoch = value,
                            b"pkgdesc" => pkgbuild.pkgdesc = value,
                            b"url" => pkgbuild.url = value,
                            b"license" => pkgbuild.license.push(value),
                            b"install" => pkgbuild.install = value,
                            b"changelog" => pkgbuild.changelog = value,
                            b"validgpgkeys" => 
                                pkgbuild.validgpgkeys.push(value),
                            b"noextract" => pkgbuild.noextract.push(value),
                            b"groups" => pkgbuild.groups.push(value),
                            b"backup" => pkgbuild.backups.push(value),
                            b"options" => pkgbuild.options.push(value),
                            b"pkgver_func" => match value {
                                b"y" => pkgbuild.pkgver_func = true,
                                b"n" => pkgbuild.pkgver_func = false,
                                _ => {
                                    log::error!("Invalid pkgver_func value: {}", 
                                       str_from_slice_u8!(line));
                                    return Err(Error::ParserScriptIllegalOutput(
                                        line.into()))
                                }
                            }
                            _ => {
                                log::error!("Line '{}' does not contain valid \
                                   key or keyword when expecting pkgbuild info", 
                                   str_from_slice_u8!(line));
                                return Err(Error::ParserScriptIllegalOutput(
                                    line.into()))
                            }
                        }
                        state = ParsingState::Pkgbuild(pkgbuild)
                    }
                }
                },
                ParsingState::Package(
                    mut pkgbuild, 
                    mut package
                ) => 
                match line {
                    b"PACKAGEARCH" => state = ParsingState::PackageArchSpecific(
                        pkgbuild, package, Default::default()),
                    b"END" => {
                        pkgbuild.pkgs.push(package);
                        state = ParsingState::Pkgbuild(pkgbuild)
                    },
                    _ => {
                        key_value_from_slice_u8!(line, key, value);
                        match key {
                            b"pkgname" => package.pkgname = value,
                            b"pkgdesc" => package.pkgdesc = value,
                            b"url" => package.url = value,
                            b"license" => package.license.push(value),
                            b"groups" => package.groups.push(value),
                            b"backup" => package.backup.push(value),
                            b"options" => package.options.push(value),
                            b"install" => package.install = value,
                            b"changelog" => package.changelog = value,
                            _ => {
                                log::error!("Line '{}' does not contain valid \
                                   key or keyword when expecting pkgbuild info", 
                                   str_from_slice_u8!(line));
                                return Err(Error::ParserScriptIllegalOutput(
                                    line.into()))
                            }
                        }
                        state = ParsingState::Package(pkgbuild, package)
                    }
                },
                ParsingState::PackageArchSpecific(
                    pkgbuild, 
                    mut package, 
                    mut arch
                ) => 
                match line {
                    b"END" => {
                        package.arches.push(arch);
                        state = ParsingState::Package(pkgbuild, package)
                    },
                    _ => {
                        key_value_from_slice_u8!(line, key, value);
                        match key {
                            b"arch" => arch.arch = value,
                            b"checkdepends" => arch.checkdepends.push(value),
                            b"depends" => arch.depends.push(value),
                            b"optdepends" => arch.optdepends.push(value),
                            b"provides" => arch.provides.push(value),
                            b"conflicts" => arch.conflicts.push(value),
                            b"replaces" => arch.replaces.push(value),
                            _ => {
                                log::error!("Line '{}' does not contain valid \
                                   key or keyword when expecting package arch \
                                   info", str_from_slice_u8!(line));
                                return Err(Error::ParserScriptIllegalOutput(
                                    line.into()))
                            }
                        }
                        state = ParsingState::PackageArchSpecific(
                            pkgbuild, package, arch)
                    }
                },
                ParsingState::PkgbuildArchSpecific(
                    mut pkgbuild,
                    mut arch
                ) =>
                match line {
                    b"END" => {
                        pkgbuild.arches.push(arch);
                        state = ParsingState::Pkgbuild(pkgbuild)
                    },
                    _ => {
                        key_value_from_slice_u8!(line, key, value);
                        match key {
                            b"arch" => arch.arch = value,
                            b"source" => arch.sources.push(value),
                            b"cksums" => arch.cksums.push(value),
                            b"md5sums" => arch.md5sums.push(value),
                            b"sha1sums" => arch.sha1sums.push(value),
                            b"sha224sums" => arch.sha224sums.push(value),
                            b"sha256sums" => arch.sha256sums.push(value),
                            b"sha384sums" => arch.sha384sums.push(value),
                            b"sha512sums" => arch.sha512sums.push(value),
                            b"b2sums" => arch.b2sums.push(value),
                            b"depends" => arch.depends.push(value),
                            b"makedepends" => arch.makedepends.push(value),
                            b"checkdepends" => arch.checkdepends.push(value),
                            b"optdepends" => arch.optdepends.push(value),
                            b"conflicts" => arch.conflicts.push(value),
                            b"provides" => arch.provides.push(value),
                            b"replaces" => arch.replaces.push(value),
                            _ => {
                                log::error!("Line '{}' does not contain valid \
                                   key or keyword when expecting pkgbuild arch \
                                   info", str_from_slice_u8!(line));
                                return Err(Error::ParserScriptIllegalOutput(
                                    line.into()))
                            }
                        }
                        state = ParsingState::PkgbuildArchSpecific(
                            pkgbuild, arch)
                    }
                },
            }
        }
        match state {
            ParsingState::None => (),
            ParsingState::Pkgbuild(pkgbuild) => 
                pkgbuilds.push(pkgbuild),
            _ => {
                log::error!("Unexpected state before finishing PKGBUILDs: {:?}",
                    state);
                return Err(Error::ParserScriptIllegalOutput(Default::default()))
            },
        }
        Ok(Self {
            entries: pkgbuilds,
        })
    }
}

/// A re-implementation of `rpmvercmp` funtion, which is used in pacman's 
/// `alpm_pkg_vercmp()` routine. This is used when comparing `PlainVersion`.
#[cfg(feature = "vercmp")]
pub fn vercmp<S1, S2>(ver1: S1, ver2: S2) -> Option<Ordering>
where
    S1: AsRef<str>,
    S2: AsRef<str>
{
    let spliter = |c: char|!c.is_ascii_alphanumeric();
    let mut segs1 = ver1.as_ref().split(spliter);
    let mut segs2 = ver2.as_ref().split(spliter);
    loop {
        let seg1 = segs1.next();
        let seg2 = segs2.next();
        if seg1.is_none() {
            if seg2.is_none() {
                return Some(Ordering::Equal)
            } else {
                return Some(Ordering::Less)
            }
        } else if seg2.is_none() {
            return Some(Ordering::Greater)
        }
        // These both cannot be None, but we still need to fight the type system
        let mut seg1 = seg1.unwrap_or("");
        let mut seg2 = seg2.unwrap_or("");
        // Compare each variant
        while let Some(c) = seg1.chars().nth(0) {
            let mut current1 = seg1;
            let mut current2 = seg2;
            let mut sub = false;
            let is_digit = c.is_ascii_digit();
            for (indic, c) in seg1.char_indices() {
                if c.is_ascii_digit() != is_digit {
                    current1 = &seg1[0..indic];
                    seg1 = &seg1[indic..];
                    sub = true;
                    break
                }
            }
            if sub {
                sub = false
            } else {
                seg1 = ""
            }
            for (indic, c) in seg2.char_indices() {
                if c.is_ascii_digit() != is_digit {
                    current2 = &seg2[0..indic];
                    seg2 = &seg2[indic..];
                    sub = true;
                    break
                }
            }
            if ! sub {
                seg2 = ""
            }
            if is_digit {
                // Prefer digit one
                if current2.is_empty() {
                    return Some(Ordering::Greater)
                }
                current1 = current1.trim_start_matches(|c: char| c == '0');
                current2 = current2.trim_start_matches(|c: char| c == '0'); 
                // Shortcut: the longer one wins
                if let Some(order) = 
                    current1.len().partial_cmp(&current2.len()) 
                {
                    if order != Ordering::Equal {
                        return Some(order)
                    }
                }
            } else if current2.is_empty() {
                // Prefer digit one
                return Some(Ordering::Less)
            }
            if let Some(order) = current1.partial_cmp(current2) {
                if order != Ordering::Equal {
                    return Some(order)
                }
            }
        }
        if ! seg1.is_empty() {
            log::error!("Version segment '{}' non empty when should be", seg1);
            return None
        }
        if ! seg2.is_empty() {
            return Some(Ordering::Less)
        }
    }
}

/// The version without ordering, the one used for package itself, but not the
/// one used when declaring dependency relationship.
#[derive(Debug, PartialEq, Eq, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PlainVersion {
    pub epoch: String,
    pub pkgver: String,
    pub pkgrel: String
}

#[cfg(feature = "vercmp")]
impl PartialOrd for PlainVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // The ALPM parseEVR() always assume at least 0 epoch
        let mut order = vercmp(
            if self.epoch.is_empty() {"0"} else {&self.epoch},
            if other.epoch.is_empty() {"0"} else {&other.epoch})?;
        if order != Ordering::Equal {
            return Some(order)
        }
        order = vercmp(&self.pkgver, &other.pkgver)?;
        if order != Ordering::Equal {
            return Some(order)
        }
        // Only compare pkgrel if they both exist
        if self.pkgrel.is_empty() || other.pkgrel.is_empty() {
            return Some(Ordering::Equal)
        }
        vercmp(&self.pkgrel, &other.pkgrel)
    }
}

#[cfg(feature = "vercmp")]
impl Ord for PlainVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        if let Some(order) = self.partial_cmp(other) {
            order
        } else {
            // Imitate the pacman vercmp behaviour, return -1 (less) for 
            // versions not comparable
            Ordering::Less
        }
    }
}

#[cfg(feature = "format")]
impl Display for PlainVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if ! self.epoch.is_empty() {
            write!(f, "{}:", self.epoch)?;
        }
        write!(f, "{}", self.pkgver)?;
        if ! self.pkgrel.is_empty() {
            write!(f, "-{}", self.pkgrel)?
        }
        Ok(())
    }
}

impl From<&str> for PlainVersion {
    fn from(value: &str) -> Self {
        let (epoch, value) = 
            match value.split_once(':') 
        {
            Some((epoch, remaining)) 
                =>(epoch.into(), remaining),
            None => (Default::default(), value),
        };
        let (pkgver, pkgrel) = 
            match value.rsplit_once('-') 
        {
            Some((pkgver,pkgrel)) => (pkgver.into(), pkgrel.into()),
            None => (value.into(), Default::default()),
        };
        Self { epoch, pkgver, pkgrel }
    }
}

impl From<&[u8]> for PlainVersion {
    fn from(value: &[u8]) -> Self {
        Self::from(str_from_slice_u8!(value))
    }
}

impl PlainVersion {
    fn from_raw(epoch: &[u8], pkgver: &[u8], pkgrel: &[u8]) -> Self {
        Self {
            epoch: string_from_slice_u8!(epoch),
            pkgver: string_from_slice_u8!(pkgver),
            pkgrel: string_from_slice_u8!(pkgrel),
        }
    }
}

/// The dependency order, comparision is not implemented yet
#[derive(Debug, PartialEq, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum DependencyOrder {
    Greater,
    GreaterOrEqual,
    Equal,
    LessOrEqual,
    Less
}

#[cfg(feature = "format")]
impl Display for DependencyOrder {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DependencyOrder::Greater => 
                write!(f, ">"),
            DependencyOrder::GreaterOrEqual => 
                write!(f, ">="),
            DependencyOrder::Equal => 
                write!(f, "="),
            DependencyOrder::LessOrEqual => 
                write!(f, "<="),
            DependencyOrder::Less => 
                write!(f, "<"),
        }
    }
}

/// The dependency version, comparision is not implemented yet
#[derive(Debug, PartialEq, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OrderedVersion {
    pub order: DependencyOrder,
    /// The version info without ordering
    pub plain: PlainVersion,
}

#[cfg(feature = "format")]
impl Display for OrderedVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.order, self.plain)
    }
}

/// A dependency
#[derive(Debug, PartialEq, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Dependency {
    pub name: String,
    pub version: Option<OrderedVersion>
}

#[cfg(feature = "format")]
impl Display for Dependency {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(version) = &self.version {
            write!(f, "{}{}", self.name, version)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

impl From<&str> for Dependency {
    fn from(value: &str) -> Self {
        if let Some((name, version)) = 
            value.split_once("=") 
        {
            if let Some((name, version)) = 
                value.split_once(">=") 
            {
                Self { name: name.into(), 
                    version: Some(OrderedVersion { 
                        order: DependencyOrder::GreaterOrEqual, 
                        plain: version.into() }) }
            } else if let Some((name, version)) = 
                value.split_once("<=") 
            {
                Self { name: name.into(), 
                    version: Some(OrderedVersion { 
                        order: DependencyOrder::LessOrEqual, 
                        plain: version.into() }) }
            } else {
                Self { name: name.into(), 
                    version: Some(OrderedVersion { 
                        order: DependencyOrder::Equal, 
                        plain: version.into() }) }
            }
        } else if let Some((name, version)) = 
            value.split_once('>') 
        {
            Self { name: name.into(), 
                version: Some(OrderedVersion { 
                    order: DependencyOrder::Greater, 
                    plain: version.into() }) }

        } else if let Some((name, version)) = 
            value.split_once('<') 
        {
            Self { name: name.into(), 
                version: Some(OrderedVersion { 
                    order: DependencyOrder::Less, 
                    plain: version.into() }) }
        } else {
            Self {name: value.into(), version: None}
        }
    }
}

impl From<&[u8]> for Dependency {
    fn from(value: &[u8]) -> Self {
        Self::from(str_from_slice_u8!(value))
    }
}

pub type MakeDependency = Dependency;
pub type CheckDependency = Dependency;

#[derive(Debug, PartialEq, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OptionalDependency {
    pub dep: Dependency,
    pub reason: String,
}

impl From<&str> for OptionalDependency {
    fn from(value: &str) -> Self {
        if let Some((dep, reason)) = 
            value.split_once(": ") 
        {
            Self {
                dep: dep.into(),
                reason: reason.into(),
            }
        } else {
            Self {
                dep: value.into(),
                reason: Default::default(),
            }
        }
    }
}

impl From<&[u8]> for OptionalDependency {
    fn from(value: &[u8]) -> Self {
        Self::from(str_from_slice_u8!(value))
    }
}

#[cfg(feature = "format")]
impl Display for OptionalDependency {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.dep)?;
        if ! self.reason.is_empty() {
            write!(f, ": {}", self.reason)?;
        }
        Ok(())
    }
}

pub type Conflict = Dependency;

#[derive(Debug, PartialEq, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Provide {
    pub name: String,
    pub version: Option<PlainVersion>
}

pub type Replace = Dependency;

#[cfg(feature = "format")]
impl Display for Provide {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(version) = &self.version {
            write!(f, "{}={}", self.name, version)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

impl TryFrom<&str> for Provide {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        if value.contains('>') ||
            value.contains('<') 
        {
            log::error!("Version string '{}' contains illegal > or <", value);
            return Err(Error::BrokenPKGBUILDs(Vec::new()))
        }
        if let Some((name, version)) = 
            value.split_once("=") 
        {
            Ok(Self { name: name.into(), 
                version: Some(version.into()) }) 
        } else {
            Ok(Self {name: value.into(), version: None})
        }
    }
}

impl TryFrom<&[u8]> for Provide {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self> {
        Self::try_from(str_from_slice_u8!(value))
    }
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PackageArchSpecific {
    pub checkdepends: Vec<CheckDependency>,
    /// The dependencies of the split package
    pub depends: Vec<Dependency>,
    pub optdepends: Vec<OptionalDependency>,
    /// What virtual packages does this package provide
    pub provides: Vec<Provide>,
    pub conflicts: Vec<Conflict>,
    pub replaces: Vec<Replace>,
}

#[derive(Debug, Clone, Default)]
pub struct MultiArch<T> {
    pub any: T,
    pub arches: BTreeMap<Architecture, T>,
}

pub fn multiarch_have_same_arches<T1, T2>(
    some: &MultiArch<T1>, other: &MultiArch<T2>
) -> bool 
{
    let this = &some.arches;
    let that = &other.arches;
    if this.is_empty() {
        that.is_empty()
    } else if that.is_empty() {
        false
    } else {
        let mut arches_this: Vec<&Architecture> = this.keys().collect();
        let mut arches_that: Vec<&Architecture> = that.keys().collect();
        arches_this.sort_unstable();
        arches_that.sort_unstable();
        arches_this == arches_that
    }
}

/// A sub-package parsed from a split-package `PKGBUILD`
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Package {
    /// The name of the split pacakge
    pub pkgname: String,
    pub pkgdesc: String,
    pub url: String,
    pub license: Vec<String>,
    pub groups: Vec<String>,
    pub backup: Vec<String>,
    pub options: Options,
    pub install: String,
    pub changelog: String,
    pub multiarch: MultiArch<PackageArchSpecific>,
}

macro_rules! pkg_iter_all_arch {
    ($pkg:ident, $var:ident, $type: ident) => {
        pub fn $var(&self) -> impl Iterator<Item = &$type> {
            let iter_any = self.multiarch.any.$var.iter();
            let iter_arches = self.multiarch.arches.iter()
                .map(|(_, pkg_arch)|pkg_arch.$var.iter()).flatten();
            iter_any.chain(iter_arches)
        }
    }
}

impl Package {
    pkg_iter_all_arch!(self, depends, Dependency);
    pkg_iter_all_arch!(self, optdepends, OptionalDependency);
    pkg_iter_all_arch!(self, provides, Provide);
    pkg_iter_all_arch!(self, conflicts, Conflict);
    pkg_iter_all_arch!(self, replaces, Replace);
}

#[cfg(feature = "format")]
fn format_write_iter<I, D>(f: &mut Formatter<'_>, array: I) 
-> std::fmt::Result 
where
    I: IntoIterator<Item = D>,
    D: Display
{
    let mut started = false;
    for item in array.into_iter() {
        if started {
            write!(f, ", {}", item)?
        } else {
            started = true;
            write!(f, "{}", item)?
        }
    }
    Ok(())
}

#[cfg(feature = "format")]
impl Display for Package {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{name: {}, depends: [", self.pkgname)?;
        format_write_iter(f, self.depends())?;
        write!(f, "], provides: [")?;
        format_write_iter(f, self.provides())?;
        write!(f, "]}}")
    }
}

/// A VSC source fragment, declared in source as `url#fragment`, usually to 
/// declare which `fragment` of the VSC source to use, e.g. commit, tag, etc
pub trait Fragment {
    /// Get the type string for the fragment, e.g. `revision`, `commit`, etc;
    /// 
    /// And get the actual value for the fragment, e.g. `master`, `main`, etc
    fn get_type_and_value(&self) -> (&str, &str);

    /// Get the type string for the fragment, e.g. `revision`, `commit`, etc;
    fn get_type(&self) -> &str {
        self.get_type_and_value().0
    }

    /// Get the actual value for the fragment, e.g. `master`, `main`, etc
    fn get_value(&self) -> &str {
        self.get_type_and_value().1
    }

    fn from_url(url: &str) -> (&str, Option<Self>)
        where Self: Sized;
}

#[cfg(feature = "format")]
impl Display for dyn Fragment {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let (ftype, value) = self.get_type_and_value();
        write!(f, "{}={}", ftype, value)
    }
}

fn split_url_fragment(url: &str) -> Option<(&str, &str, &str)> {
    if let Some((prefix, fragment)) = 
        url.split_once('#') 
    {
        if let Some((key, value)) = 
            fragment.split_once('=') 
        {
            return Some((prefix, key, value))
        }
    }
    None
}

fn split_url_fragment_no_query(url: &str) -> Option<(&str, &str, &str)> {
    if let Some((mut prefix, mut fragment)) = 
        url.split_once('#') 
    {
        if let Some((nfragment, _)) = fragment.split_once('?'){
            fragment = nfragment
        }
        if let Some((key, value)) = 
            fragment.split_once('=') 
        {
            if let Some((url, _)) = prefix.split_once('?') {
                prefix = url
            }
            return Some((prefix, key, value))
        }
    }
    None
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum BzrSourceFragment {
    Revision(String)
}

impl Fragment for BzrSourceFragment {
    fn get_type_and_value(&self) -> (&str, &str) {
        match self {
            BzrSourceFragment::Revision(revision) 
                => ("revision", revision),
        }
    }

    fn from_url(url: &str) -> (&str, Option<Self>) {
        if let Some((url, key, value)) = 
            split_url_fragment(url) 
        {
            match key {
                "revision" => (url, Some(Self::Revision(value.into()))),
                _ => (url, None),
            }
        } else {
            (url, None)
        }
    }
}

#[cfg(feature = "format")]
impl Display for BzrSourceFragment {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        (self as &dyn Fragment).fmt(f)
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum FossilSourceFragment {
    Branch(String),
    Commit(String),
    Tag(String)
}

impl Fragment for FossilSourceFragment {
    fn get_type_and_value(&self) -> (&str, &str) {
        match self {
            FossilSourceFragment::Branch(branch) 
                => ("branch", branch),
            FossilSourceFragment::Commit(commit) 
                => ("commit", commit),
            FossilSourceFragment::Tag(tag) 
                => ("tag", tag),
        }
    }

    fn from_url(url: &str) -> (&str, Option<Self>) {
        if let Some((url, key, value)) = 
            split_url_fragment(url) 
        {
            match key {
                "branch" => (url, Some(Self::Branch(value.into()))),
                "commit" => (url, Some(Self::Commit(value.into()))),
                "tag" => (url, Some(Self::Tag(value.into()))),
                _ => (url, None),
            }
        } else {
            (url, None)
        }
    }
}

#[cfg(feature = "format")]
impl Display for FossilSourceFragment {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        (self as &dyn Fragment).fmt(f)
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum GitSourceFragment {
    Branch(String),
    Commit(String),
    Tag(String)
}

impl Fragment for GitSourceFragment {
    fn get_type_and_value(&self) -> (&str, &str) {
        match self {
            GitSourceFragment::Branch(branch) 
                => ("branch", branch),
            GitSourceFragment::Commit(commit) 
                => ("commit", commit),
            GitSourceFragment::Tag(tag) 
                => ("tag", tag),
        }
    }
    
    fn from_url(url: &str) -> (&str, Option<Self>) {
        if let Some((url, key, value)) = 
            split_url_fragment_no_query(url) 
        {
            match key {
                "branch" => (url, Some(Self::Branch(value.into()))),
                "commit" => (url, Some(Self::Commit(value.into()))),
                "tag" => (url, Some(Self::Tag(value.into()))),
                _ => (url, None),
            }
        } else {
            (url, None)
        }
    }
}

#[cfg(feature = "format")]
impl Display for GitSourceFragment {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        (self as &dyn Fragment).fmt(f)
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum HgSourceFragment {
    Branch(String),
    Revision(String),
    Tag(String)
}

impl Fragment for HgSourceFragment {
    fn get_type_and_value(&self) -> (&str, &str) {
        match self {
            HgSourceFragment::Branch(branch) 
                => ("branch", branch),
            HgSourceFragment::Revision(revision)
                => ("revision", revision),
            HgSourceFragment::Tag(tag) 
                => ("tag", tag),
        }
    }

    fn from_url(url: &str) -> (&str, Option<Self>) {
        if let Some((url, key, value)) = 
            split_url_fragment(url) 
        {
            match key {
                "branch" => (url, Some(Self::Branch(value.into()))),
                "revision" => (url, Some(Self::Revision(value.into()))),
                "tag" => (url, Some(Self::Tag(value.into()))),
                _ => (url, None),
            }
        } else {
            (url, None)
        }
    }
}

#[cfg(feature = "format")]
impl Display for HgSourceFragment {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        (self as &dyn Fragment).fmt(f)
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SvnSourceFragment {
    Revision(String)
}

impl Fragment for SvnSourceFragment {
    fn get_type_and_value(&self) -> (&str, &str) {
        match self {
            SvnSourceFragment::Revision(revision) 
                => ("revision", revision),
        }
    }

    fn from_url(url: &str) -> (&str, Option<Self>) {
        if let Some((url, key, value)) = 
            split_url_fragment(url) 
        {
            match key {
                "revision" => (url, Some(Self::Revision(value.into()))),
                _ => (url, None),
            }
        } else {
            (url, None)
        }
    }
}

#[cfg(feature = "format")]
impl Display for SvnSourceFragment {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        (self as &dyn Fragment).fmt(f)
    }
}

#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SourceProtocol {
    #[default]
    Unknown,
    Local,
    File,
    Ftp,
    Http,
    Https,
    Rsync,
    Bzr {
        fragment: Option<BzrSourceFragment>,
    },
    Fossil {
        fragment: Option<FossilSourceFragment>,
    },
    Git {
        fragment: Option<GitSourceFragment>,
        signed: bool
    },
    Hg {
        fragment: Option<HgSourceFragment>,
    },
    Svn {
        fragment: Option<SvnSourceFragment>,
    }
}

#[cfg(feature = "format")]
impl Display for SourceProtocol {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceProtocol::Unknown => write!(f, "unknown")?,
            SourceProtocol::Local => write!(f, "local")?,
            SourceProtocol::File => write!(f, "file")?,
            SourceProtocol::Ftp => write!(f, "ftp")?,
            SourceProtocol::Http => write!(f, "http")?,
            SourceProtocol::Https => write!(f, "https")?,
            SourceProtocol::Rsync => write!(f, "rsync")?,
            SourceProtocol::Bzr { fragment } => {
                write!(f, "bzr")?;
                if let Some(fragment) = fragment {
                    write!(f, "({})", fragment)?
                }
            },
            SourceProtocol::Fossil { fragment } 
            => {
                write!(f, "fossil")?;
                if let Some(fragment) = fragment {
                    write!(f, "({})", fragment)?
                }
            },
            SourceProtocol::Git { 
                fragment, signed } => 
            {
                write!(f, "git")?;
                if let Some(fragment) = fragment {
                    if *signed {
                        write!(f, "({}, signed)", fragment)?
                    } else {
                        write!(f, "({})", fragment)?
                    }
                } else if *signed {
                    write!(f, "(signed)")?
                }
            },
            SourceProtocol::Hg { fragment } => {
                write!(f, "hg")?;
                if let Some(fragment) = fragment {
                    write!(f, "({})", fragment)?
                }
            },
            SourceProtocol::Svn { fragment } => {
                write!(f, "svn")?;
                if let Some(fragment) = fragment {
                    write!(f, "({})", fragment)?
                }
            },
        }
        Ok(())
    }
}

#[cfg(feature = "format")]
impl SourceProtocol {
    fn get_proto_str(&self) -> &'static str {
        match self {
            SourceProtocol::Unknown => "unknown",
            SourceProtocol::Local => "local",
            SourceProtocol::File => "file",
            SourceProtocol::Ftp => "ftp",
            SourceProtocol::Http => "http",
            SourceProtocol::Https => "https",
            SourceProtocol::Rsync => "rsync",
            SourceProtocol::Bzr { fragment: _ } => "bzr",
            SourceProtocol::Fossil { fragment: _ } => "fossil",
            SourceProtocol::Git { fragment: _, signed: _ } => "git",
            SourceProtocol::Hg { fragment: _ } => "hg",
            SourceProtocol::Svn { fragment: _ } => "svn",
        }
    }
}

#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Source {
    /// The local file name
    pub name: String,
    /// The actual URL, i.e. the one used to initialize connections, could be
    /// different from the one defined in `source=()`
    pub url: String,
    /// The protocol, and the protocol-specific data
    pub protocol: SourceProtocol,
}

#[cfg(feature = "format")]
impl Display for Source {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{name: {}, url: {}, protocol: {}}}",
                    self.name, self.url, self.protocol)
    }
}

impl From<&str> for Source {
    fn from(definition: &str) -> Self {
        let mut source = Self::default();
        let mut url = match definition.split_once("::") {
            Some((name, url)) => {
                source.name = name.into();
                url
            },
            None => definition,
        };
        source.protocol = 
            if let Some((mut proto, _)) = 
                url.split_once("://") 
            {
                if let Some((proto_actual, _)) = 
                    proto.split_once('+') 
                {
                    // E.g. git+https://github.com/7Ji/ampart.git
                    // proto would be git, its length is 3, then url would be
                    // https://github.com/7Ji/ampart.git, it's a substr from 4
                    url = &url[proto_actual.len() + 1..];
                    proto = proto_actual;
                };
                match proto {
                    "file" => SourceProtocol::File,
                    "ftp" => SourceProtocol::Ftp,
                    "http" => SourceProtocol::Http,
                    "https" => SourceProtocol::Https,
                    "rsync" => SourceProtocol::Rsync,
                    "bzr" => {
                        let (urln, fragment) 
                            = BzrSourceFragment::from_url(url);
                        url = urln;
                        SourceProtocol::Bzr { fragment }
                    },
                    "fossil" => {
                        let (urln, fragment) 
                            = FossilSourceFragment::from_url(url);
                        url = urln;
                        SourceProtocol::Fossil { fragment }
                    },
                    "git" => {
                        let (urln, fragment) 
                            = GitSourceFragment::from_url(url);
                        url = urln;
                        SourceProtocol::Git { fragment, 
                            signed: url.contains("?signed")}
                    },
                    "hg" => {
                        let (urln, fragment) 
                            = HgSourceFragment::from_url(url);
                        url = urln;
                        SourceProtocol::Hg { fragment }

                    },
                    "svn" => {
                        let (urln, fragment) 
                            = SvnSourceFragment::from_url(url);
                        url = urln;
                        SourceProtocol::Svn { fragment }

                    },
                    _ => {
                        log::warn!("Unknown protocol '{}'", proto);
                        SourceProtocol::Unknown
                    }
                }
            } else { // No scheme, local file
                SourceProtocol::Local
            };
        source.url = url.into();
        if source.name.is_empty() {
            source.name = source.get_url_name()
        }
        source
    }
}

impl From<&[u8]> for Source {
    fn from(value: &[u8]) -> Self {
        str_from_slice_u8!(value).into()
    }
}

impl Source {
    /// Create a new `Source` from the definition used in `PKGBUILD`
    pub fn new<S: AsRef<str>>(definition: S) -> Self {
        definition.as_ref().into()
    }
    /// Generate name from the url
    pub fn get_url_name(&self) -> String {
        let mut name: String = 
            match self.url.rsplit_once('/') {
                Some((_, name)) => name.into(),
                None => (&self.url).into(),
            };
        match &self.protocol {
            SourceProtocol::Bzr { fragment: _ } => 
                if let Some((_, rname)) = name.split_once("lp:") 
                {
                    name = rname.into()
                },
            SourceProtocol::Fossil { fragment: _ } => 
                name.push_str(".fossil"),
            SourceProtocol::Git { fragment: _, signed: _ } => 
                if let Some(lname) = name.strip_suffix(".git") {
                    let len = lname.len();
                    name.truncate(len);
                    name.shrink_to(len)
                }
            _ => (),
        }
        name
    }

    #[cfg(feature = "format")]
    /// Convert to the format `PKGBUILD` uses in the `source` array
    pub fn get_pkgbuild_source(&self) -> String {
        let mut raw = String::new();
        let auto_name = self.get_url_name();
        if auto_name != self.name {
            raw.push_str(&self.name);
            raw.push_str("::")
        }
        let proto_url = match self.url.split_once("://") {
            Some((proto, _)) => proto,
            None => "",
        };
        let proto_actual = self.protocol.get_proto_str();
        match &self.protocol {
            SourceProtocol::Unknown | SourceProtocol::Local => (),
            _ =>
                if proto_actual != proto_url {
                    raw.push_str(proto_actual);
                    raw.push('+');
                }
        }
        raw.push_str(&self.url);
        macro_rules! push_fragment {
            ($fragment: ident) => {
                if let Some(fragment) = $fragment {
                    raw.push_str(&format!("#{}", fragment))
                }
            };
        }
        match &self.protocol {
            SourceProtocol::Bzr { fragment } => 
                push_fragment!(fragment),
            SourceProtocol::Fossil { fragment } => 
                push_fragment!(fragment),
            SourceProtocol::Git { fragment, signed } => {
                push_fragment!(fragment);
                if *signed {
                    raw.push_str("?signed")
                }
            },
            SourceProtocol::Hg { fragment } => 
                push_fragment!(fragment),
            SourceProtocol::Svn { fragment } => 
                push_fragment!(fragment),
            _ => (),
        };
        raw
    }
}

pub type Cksum = u32;
pub type Md5sum = [u8; 16];
pub type Sha1sum = [u8; 20];
pub type Sha224sum = [u8; 28];
pub type Sha256sum = [u8; 32];
pub type Sha384sum = [u8; 48];
pub type Sha512sum = [u8; 64];
pub type B2sum = [u8; 64];

#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SourceWithChecksum {
    pub source: Source,
    pub cksum: Option<Cksum>,
    pub md5sum: Option<Md5sum>,
    pub sha1sum: Option<Sha1sum>,
    pub sha224sum: Option<Sha224sum>,
    pub sha256sum: Option<Sha256sum>,
    pub sha384sum: Option<Sha384sum>,
    pub sha512sum: Option<Sha512sum>,
    pub b2sum: Option<B2sum>,
}

#[cfg(feature = "format")]
fn write_byte_iter<I>(f: &mut Formatter<'_>, bytes: I) -> std::fmt::Result 
where
    I: IntoIterator<Item = u8>
{
    for byte in bytes.into_iter() {
        write!(f, "{:02x}", byte)?
    }
    Ok(())
}

#[cfg(feature = "format")]
impl Display for SourceWithChecksum {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{source: {}", self.source)?;
        if let Some(cksum) = self.cksum {
            write!(f, ", cksum: {}", cksum)?
        }
        macro_rules! write_cksum {
            ($($cksum: ident), +) => {
                $(
                    if let Some($cksum) = self.$cksum {
                        write!(f, ", {}: ", stringify!($cksum))?;
                        write_byte_iter(f, $cksum)?
                    }
                )+
            };
        }
        write_cksum!(md5sum, sha1sum, sha224sum, sha256sum, 
            sha384sum, sha512sum, b2sum);
        write!(f, "}}")
    }
}

// #[derive(Debug, Default, Clone)]
// #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
// pub enum PkgbuildOptionKey {
//     #[default]
//     Strip,
//     Docs,
//     LibTool,
//     StaticLibs,
//     EmptyDirs,
//     ZipMan,
//     CCache,
//     DistCC,
//     BuildFlags,
//     MakeFlags,
//     Debug,
//     Lto,
// }

// impl<'a> TryFrom<&'a [u8]> for PkgbuildOptionKey {
//     type Error = Error;

//     fn try_from(value: &'a [u8]) -> Result<Self> {
//         Ok(match value {
//             b"strip" => Self::Strip,
//             b"docs" => Self::Docs,
//             b"libtool" => Self::LibTool,
//             b"staticlibs" => Self::StaticLibs,
//             b"emptydirs" => Self::EmptyDirs,
//             b"zipman" => Self::ZipMan,
//             b"ccache" => Self::CCache,
//             b"distcc" => Self::DistCC,
//             b"buildflags" => Self::BuildFlags,
//             b"makeflags" => Self::MakeFlags,
//             b"debug" => Self::Debug,
//             b"lto" => Self::Lto,
//             _ => {
//                 log::error!("Unknown option '{}'", str_from_slice_u8!(value));
//                 return Err(Error::BrokenPKGBUILDs(Default::default()))
//             }
//         })
//     }
// }

// impl AsRef<str> for PkgbuildOptionKey {
//     fn as_ref(&self) -> &str {
//         match self {
//             PkgbuildOptionKey::Strip => "strip",
//             PkgbuildOptionKey::Docs => "docs",
//             PkgbuildOptionKey::LibTool => "libtool",
//             PkgbuildOptionKey::StaticLibs => "staticlibs",
//             PkgbuildOptionKey::EmptyDirs => "emptydirs",
//             PkgbuildOptionKey::ZipMan => "zipman",
//             PkgbuildOptionKey::CCache => "ccache",
//             PkgbuildOptionKey::DistCC => "distcc",
//             PkgbuildOptionKey::BuildFlags => "buildflags",
//             PkgbuildOptionKey::MakeFlags => "makeflags",
//             PkgbuildOptionKey::Debug => "debug",
//             PkgbuildOptionKey::Lto => "lto",
//         }
//     }
// }

// impl Display for PkgbuildOptionKey {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{}", self.as_ref())
//     }
// }


// #[derive(Debug, Default, Clone)]
// #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
// pub struct PkgbuildOption {
//     pub key: PkgbuildOptionKey,
//     pub value: bool
// }

// impl Display for PkgbuildOption {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         if ! self.value {
//             write!(f, "!")?;
//         }
//         write!(f, "{}", self.key.as_ref())
//     }
// }

// #[derive(Debug, Default, Clone)]
// #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
// /// This is not clean, but we use this to respect the original order
// pub struct PkgbuildOptions {
//     options: Vec<PkgbuildOption>
// }

// impl<'a> From<&Vec<&'a [u8]>> for PkgbuildOptions {
//     fn from(value: &Vec<&'a [u8]>) -> Self {
//         let mut options = Self::default();
//         for item in value.iter() {
//             if item.is_empty() { 
//                 continue 
//             }
//             let mut item = *item;
//             let enable = item[0] != b'!';
//             if ! enable { 
//                 item = &item[1..]; 
//                 if item.is_empty() {
//                     continue 
//                 }
//             }
//             if let Ok(key) = item.try_into() {
//                 options.options.push(PkgbuildOption { key, value: enable })
//             }
//         }
//         options
//     }
// }


#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Options {
    pub strip: Option<bool>,
    pub docs: Option<bool>,
    pub libtool: Option<bool>,
    pub staticlibs: Option<bool>,
    pub emptydirs: Option<bool>,
    pub zipman: Option<bool>,
    pub ccache: Option<bool>,
    pub distcc: Option<bool>,
    pub buildflags: Option<bool>,
    pub makeflags: Option<bool>,
    pub debug: Option<bool>,
    pub lto: Option<bool>,
}

// impl From<&PkgbuildOptions> for Options {
//     fn from(value: &PkgbuildOptions) -> Self {
//         let mut options = Self::default();
//         for option in value.options.iter() {
//             let value = Some(option.value);
//             match option.key {
//                 PkgbuildOptionKey::Strip => options.strip = value,
//                 PkgbuildOptionKey::Docs => options.docs = value,
//                 PkgbuildOptionKey::LibTool => options.libtool = value,
//                 PkgbuildOptionKey::StaticLibs => options.staticlibs = value,
//                 PkgbuildOptionKey::EmptyDirs => options.emptydirs = value,
//                 PkgbuildOptionKey::ZipMan => options.zipman = value,
//                 PkgbuildOptionKey::CCache => options.ccache = value,
//                 PkgbuildOptionKey::DistCC => options.distcc = value,
//                 PkgbuildOptionKey::BuildFlags => options.buildflags = value,
//                 PkgbuildOptionKey::MakeFlags => options.makeflags = value,
//                 PkgbuildOptionKey::Debug => options.debug = value,
//                 PkgbuildOptionKey::Lto => options.lto = value,
//             }
//         }
//         options
//     }
// }

impl<'a> From<&Vec<&'a [u8]>> for Options {
    fn from(value: &Vec<&'a [u8]>) -> Self {
        let mut options = Self::default();
        for item in value.iter() {
            if item.is_empty() { 
                continue 
            }
            let mut item = *item;
            let enable = item[0] != b'!';
            if ! enable { 
                item = &item[1..]; 
                if item.is_empty() {
                    continue 
                }
            }
            match item {
                b"strip" => options.strip = Some(enable),
                b"docs" => options.docs = Some(enable),
                b"libtool" => options.libtool = Some(enable),
                b"staticlibs" => options.staticlibs = Some(enable),
                b"emptydirs" => options.emptydirs = Some(enable),
                b"zipman" => options.zipman = Some(enable),
                b"ccache" => options.ccache = Some(enable),
                b"distcc" => options.distcc = Some(enable),
                b"buildflags" => options.buildflags = Some(enable),
                b"makeflags" => options.makeflags = Some(enable),
                b"debug" => options.debug = Some(enable),
                b"lto" => options.lto = Some(enable),
                _ => log::warn!("Unknown option {}", str_from_slice_u8!(item)),
            }
        }
        options
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Architecture {
    #[default]
    // Arch Linux specific
    X86_64,
    // Arch Linux ARM specific
    Aarch64,
    Armv7h,
    // Arch Linux RSIC-V specific
    Riscv64,
    Other(String)
}

impl From<&[u8]> for Architecture {
    fn from(value: &[u8]) -> Self {
        Self::from(str_from_slice_u8!(value))
    }
}

impl From<&str> for Architecture {
    fn from(value: &str) -> Self {
        let arch = value.to_lowercase();
        match arch.as_str() {
            // "any" => Self::Any,
            "x86_64" => Self::X86_64,
            "aarch64" => Self::Aarch64,
            "armv7h" => Self::Armv7h,
            "riscv64" => Self::Riscv64,
            _ => Self::Other(arch)
        }
    }
}

impl AsRef<str> for Architecture {
    fn as_ref(&self) -> &str {
        match self {
            // Architecture::Any => "any",
            Architecture::X86_64 => "x86_64",
            Architecture::Aarch64 => "aarch64",
            Architecture::Armv7h => "armv7h",
            Architecture::Riscv64 => "riscv64",
            Architecture::Other(arch) => &arch,
        }
    }
}

impl Display for Architecture {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}


/// A `PKGBUILD`'s arch-specific variables
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PkgbuildArchSpecific {
    pub sources_with_checksums: Vec<SourceWithChecksum>,
    pub depends: Vec<Dependency>,
    pub makedepends: Vec<MakeDependency>,
    pub checkdepends: Vec<CheckDependency>,
    pub optdepends: Vec<OptionalDependency>,
    pub conflicts: Vec<Conflict>,
    pub provides: Vec<Provide>,
    pub replaces: Vec<Replace>,
}

// #[derive(Default)]
// pub struct SourcesAndChecksums<'a> {
//     sources: Vec<&'a Source>,
//     cksums: Vec<Option<Cksum>>,
//     md5sums: Vec<&'a Option<Md5sum>>,
//     sha1sums: Vec<&'a Option<Sha1sum>>,
//     sha224sums: Vec<&'a Option<Sha224sum>>,
//     sha256sums: Vec<&'a Option<Sha256sum>>,
//     sha384sums: Vec<&'a Option<Sha384sum>>,
//     sha512sums: Vec<&'a Option<Sha512sum>>,
//     b2sums: Vec<&'a Option<B2sum>>,
// }

// impl<'a> From<&'a PkgbuildArchSpecific> for SourcesAndChecksums<'a> {
//     fn from(value: &'a PkgbuildArchSpecific) -> Self {
//         let mut sources_and_checksums = Self::default();
//         let mut cksums = false;
//         let mut md5sums = false;

//         for source_with_checksum in value.sources_with_checksums.iter() {
//             sources_and_checksums.sources.push(&source_with_checksum.source);
//             if let Some(cksum) = source_with_checksum.cksum {
//                 sources_and_checksums.cksums.push(cksum)
//             }
//             macro_rules! push_and_check {
//                 ($checksum: ident, $checksums: ident) => {

//                     if let Some($checksum) = &source_with_checksum.$checksum {
//                         sources_and_checksums.$checksums.push($checksum)
//                     }
//                 };
//             }
//             push_if_some!(md5sum, md5sums);
//             push_if_some!(sha1sum, sha1sums);
//             push_if_some!(sha224sum, sha224sums);
//             push_if_some!(sha256sum, sha256sums);
//             push_if_some!(sha384sum, sha384sums);
//             push_if_some!(sha512sum, sha512sums);
//             push_if_some!(b2sum, b2sums);
//         }
//         sources_and_checksums
//     }
// }

// impl PkgbuildArchSpecific {
//     pub fn sources_and_checksums(&self) -> SourcesAndChecksums {
//         self.into()
//     }
// }

/// A `PKGBUILD` that could potentially have multiple split-packages
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Pkgbuild {
    pub pkgbase: String,
    pub pkgs: Vec<Package>,
    pub version: PlainVersion,
    pub pkgdesc: String,
    pub url: String,
    pub license: Vec<String>,
    pub install: String,
    pub changelog: String,
    pub validpgpkeys: Vec<String>,
    pub noextract: Vec<String>,
    pub groups: Vec<String>,
    pub multiarch: MultiArch<PkgbuildArchSpecific>,
    pub backup: Vec<String>,
    pub options: Options,
    pub pkgver_func: bool,
}

#[cfg(feature = "format")]
impl Display for Pkgbuild {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{base: {}, pkgs: [", self.pkgbase)?;
        format_write_iter(f, &self.pkgs)?;
        write!(f, "], version: {}", self.version)?;
        if self.pkgver_func {
            write!(f, " (has pkgver func)")?;
        }
        write!(f, ", depends: [")?;
        format_write_iter(f, self.depends())?;
        write!(f, "], makedepends: [")?;
        format_write_iter(f, self.makedepends())?;
        write!(f, "], provides: [")?;
        format_write_iter(f, self.provides())?;
        write!(f, "], sources_with_checksums: [")?;
        format_write_iter(f, self.sources_with_checksums())?;
        write!(f, "]}}")
    }
}

#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Pkgbuilds {
    entries: Vec<Pkgbuild>
}

#[cfg(feature = "format")]
impl Display for Pkgbuilds {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for pkgbuild in self.entries.iter() {
            writeln!(f, "{}", pkgbuild)?
        }
        Ok(())
    }
}

fn vec_items_from_vec_items<'a, I1, I2>(items: &'a Vec<&'a I2>) -> Vec<I1>
where
    I1: From<&'a I2>,
    I2: ?Sized
{
    items.iter().map(|item|I1::from(*item)).collect()
}

fn vec_items_try_from_vec_items<'a, I1, I2>(items: &'a Vec<&'a I2>) 
-> Result<Vec<I1>>
where
    I1: TryFrom<&'a I2>, Error: From<<I1 as TryFrom<&'a I2>>::Error>,
    I2: ?Sized
{
    let mut converted = Vec::new();
    for item in items.iter() {
        converted.push(I1::try_from(*item)?)
    }
    Ok(converted)
}

impl TryFrom<&PackageArchitectureParsing<'_>> for PackageArchSpecific {
    type Error = Error;

    fn try_from(value: &PackageArchitectureParsing<'_>) -> Result<Self> {
        let provides = 
            vec_items_try_from_vec_items(&value.provides)?;
        Ok(Self {
            checkdepends: vec_items_from_vec_items(&value.checkdepends),
            depends: vec_items_from_vec_items(&value.depends),
            optdepends: vec_items_from_vec_items(&value.optdepends),
            provides,
            conflicts: vec_items_from_vec_items(&value.conflicts),
            replaces: vec_items_from_vec_items(&value.replaces),
        })
    }   
}

fn vec_string_from_vec_slice_u8<'a>(vec: &Vec<&'a [u8]>) -> Vec<String> {
    vec.iter().map(|item|string_from_slice_u8!(*item)).collect()
}

impl TryFrom<&PackageParsing<'_>> for Package {
    type Error = Error;

    fn try_from(value: &PackageParsing) -> Result<Self> {
        let mut multiarch 
            = MultiArch::default();
        for arch in value.arches.iter() {
            let arch_value = 
                PackageArchSpecific::try_from(arch)?;
            if arch.arch == b"any" {
                multiarch.any = arch_value;
                continue
            }
            if let Some(_) = multiarch.arches.insert(
                Architecture::from(arch.arch), arch_value) 
            {
                log::error!("Duplicated architecture {}", 
                    str_from_slice_u8!(arch.arch));
                return Err(Error::BrokenPKGBUILDs(Default::default()))
            }
        }
        Ok(Self { 
            pkgname: string_from_slice_u8!(value.pkgname),
            pkgdesc: string_from_slice_u8!(value.pkgdesc), 
            url: string_from_slice_u8!(value.url),
            license: vec_string_from_vec_slice_u8(&value.license),
            groups: vec_string_from_vec_slice_u8(&value.groups),
            backup: vec_string_from_vec_slice_u8(&value.backup),
            options: (&value.options).into(),
            install: string_from_slice_u8!(value.install),
            changelog: string_from_slice_u8!(value.changelog),
            multiarch
         })
    }
}



impl TryFrom<&PkgbuildArchitectureParsing<'_>> for PkgbuildArchSpecific {
    type Error = Error;

    fn try_from(value: &PkgbuildArchitectureParsing) -> Result<Self> {
        let mut sources_with_checksums = Vec::new();
        if ! value.sources.is_empty() {
            let len = value.sources.len();
            macro_rules! len_mismatch {
                ($value:ident, $sums:ident, $len:ident) => {
                    ! $value.$sums.is_empty() && $value.$sums.len() != $len
                };
            }
            if len_mismatch!(value, cksums, len) ||
                len_mismatch!(value, md5sums, len) ||
                len_mismatch!(value, sha1sums, len) ||
                len_mismatch!(value, sha224sums, len) ||
                len_mismatch!(value, sha256sums, len) ||
                len_mismatch!(value, sha384sums, len) ||
                len_mismatch!(value, sha512sums, len) ||
                len_mismatch!(value, b2sums, len)
            {
                log::error!("Lengths of sources and checksums mismatch, \
                    sources: {}, cksums: {}, md5sums: {}, sha1sums: {} \
                    sha224sums: {}, sha256sums: {}, sha384sums: {} \
                    sha512sums: {}, b2sums: {}",
                    value.sources.len(), value.cksums.len(), value.md5sums.len(),
                    value.sha1sums.len(), value.sha224sums.len(), 
                    value.sha256sums.len(), value.sha384sums.len(),
                    value.sha512sums.len(), value.b2sums.len());
                return Err(Error::BrokenPKGBUILDs(Default::default()))
            }
            for (id, source) in value.sources.iter().enumerate(){
                let mut source_with_checksum = 
                    SourceWithChecksum::default();
                source_with_checksum.source = (*source).into();
                if let Some(cksum) = value.cksums.get(id) {
                    source_with_checksum.cksum = if cksum == b"SKIP" {
                        None
                    } else {
                        String::from_utf8_lossy(cksum).parse().ok()
                    }
                }
                macro_rules! hash_sum_from_hex {
                    ($sum:ident, $sums:ident) => {
                        if let Some($sum) = value.$sums.get(id) {
                            source_with_checksum.$sum = if $sum == b"SKIP" {
                                None
                            } else {
                                FromHex::from_hex($sum).ok()
                            }
                        }                        
                    };
                }
                hash_sum_from_hex!(md5sum, md5sums);
                hash_sum_from_hex!(sha1sum, sha1sums);
                hash_sum_from_hex!(sha224sum, sha224sums);
                hash_sum_from_hex!(sha256sum, sha256sums);
                hash_sum_from_hex!(sha384sum, sha384sums);
                hash_sum_from_hex!(sha512sum, sha512sums);
                hash_sum_from_hex!(b2sum, b2sums);
                sources_with_checksums.push(source_with_checksum)
            }
        }
        let provides = 
            vec_items_try_from_vec_items(&value.provides)?;
        Ok (Self {
            sources_with_checksums,
            depends: vec_items_from_vec_items(&value.depends),
            makedepends: vec_items_from_vec_items(&value.makedepends),
            checkdepends: vec_items_from_vec_items(&value.checkdepends),
            optdepends: vec_items_from_vec_items(&value.optdepends),
            conflicts: vec_items_from_vec_items(&value.conflicts),
            provides,
            replaces: vec_items_from_vec_items(&value.replaces),
        })
    }
}



impl TryFrom<&PkgbuildParsing<'_>> for Pkgbuild {
    type Error = Error;

    fn try_from(value: &PkgbuildParsing) -> Result<Self> {
        let mut pkgs = Vec::new();
        for pkg in value.pkgs.iter() {
            pkgs.push(pkg.try_into()?)
        }
        let mut multiarch = MultiArch::default();
        for arch in value.arches.iter() {
            let arch_value = 
                PkgbuildArchSpecific::try_from(arch)?;
            if arch.arch == b"any" {
                multiarch.any = arch_value;
                continue
            }
            if let Some(_) = 
                multiarch.arches.insert(Architecture::from(arch.arch), arch_value) 
            {
                log::error!("Duplicated architecture {}", 
                    str_from_slice_u8!(arch.arch));
                return Err(Error::BrokenPKGBUILDs(Default::default()))
            }
        }
        Ok(Self {
            pkgbase: string_from_slice_u8!(value.pkgbase),
            pkgs,
            version: PlainVersion::from_raw(
                value.epoch, value.pkgver, value.pkgrel),
            pkgdesc: string_from_slice_u8!(value.pkgdesc),
            url: string_from_slice_u8!(value.url),
            license: vec_string_from_vec_slice_u8(&value.license),
            install: string_from_slice_u8!(value.install),
            changelog: string_from_slice_u8!(value.changelog),
            validpgpkeys: vec_string_from_vec_slice_u8(&value.validgpgkeys),
            noextract: vec_string_from_vec_slice_u8(&value.noextract),
            groups: vec_string_from_vec_slice_u8(&value.groups),
            multiarch,
            backup: vec_string_from_vec_slice_u8(&value.backups),
            options: (&value.options).into(),
            pkgver_func: value.pkgver_func
        })
    }
}

impl TryFrom<&PkgbuildsParsing<'_>> for Pkgbuilds {
    type Error = Error;

    fn try_from(value: &PkgbuildsParsing<'_>) -> Result<Self> {
        let mut entries = Vec::new();
        for entry in value.entries.iter() {
            entries.push(entry.try_into()?)
        }
        Ok(Self {entries})
    }
}

impl Pkgbuild {
    pkg_iter_all_arch!(self, sources_with_checksums, SourceWithChecksum);
    pkg_iter_all_arch!(self, depends, Dependency);
    pkg_iter_all_arch!(self, makedepends, MakeDependency);
    pkg_iter_all_arch!(self, checkdepends, CheckDependency);
    pkg_iter_all_arch!(self, optdepends, OptionalDependency);
    pkg_iter_all_arch!(self, conflicts, Conflict);
    pkg_iter_all_arch!(self, provides, Provide);
    pkg_iter_all_arch!(self, replaces, Replace);

    /// Get a result similar to `makepkg --printsrcinfo`, useful for formatting
    #[cfg(feature = "srcinfo")]
    pub fn srcinfo<'a>(&'a self) -> Srcinfo<'a> {
        Srcinfo { pkgbuild: self }
    }

    // /// Get a flattened list of options, note it would be impossible to go back
    // /// to the original order of options from only the result options.
    // pub fn options(&self) -> Options {
    //     (&self.options).into()
    // }
}

#[cfg(feature = "srcinfo")]
pub struct Srcinfo<'a> {
    pub pkgbuild: &'a Pkgbuild
}

#[cfg(feature = "srcinfo")]
impl<'a> Display for Srcinfo<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        fn writeln_indented_str<S: AsRef<str>>(
            f: &mut Formatter<'_>, title: &str, content: S
        ) -> std::fmt::Result 
        {
            let content = content.as_ref();
            if content.is_empty() { return Ok(()) }
            writeln!(f, "\t{} = {}", title, content)
        }
        fn writeln_indented_display<D: Display>(
            f: &mut Formatter<'_>, title: &str, content: D
        ) -> std::fmt::Result 
        {
            writeln_indented_str(f, title, &content.to_string())
        }
        let pkgbuild = self.pkgbuild;
        writeln!(f, "pkgbase = {}", pkgbuild.pkgbase)?;
        writeln_indented_str(f, "pkgdesc", &pkgbuild.pkgdesc)?;
        writeln_indented_str(f, "pkgver", &pkgbuild.version.pkgver)?;
        writeln_indented_str(f, "pkgrel", &pkgbuild.version.pkgrel)?;
        writeln_indented_str(f, "epoch", &pkgbuild.version.epoch)?;
        writeln_indented_str(f, "url", &pkgbuild.url)?;
        writeln_indented_str(f, "install", &pkgbuild.install)?;
        writeln_indented_str(f, "changelog", &pkgbuild.changelog)?;
        if pkgbuild.multiarch.arches.is_empty() {
            writeln_indented_str(f, "arch", "any")?;
        } else {
            for (arch, _) in pkgbuild.multiarch.arches.iter() {
                writeln_indented_str(f, "arch", arch)?;
            }
        }
        fn writelns_indented_iter_str<I, S>(
            f: &mut Formatter<'_>, title: &str, contents: I
        ) -> std::fmt::Result
        where
            I: IntoIterator<Item = S>,
            S: AsRef<str>
        {
            for content in contents.into_iter() {
                writeln_indented_str(f, title, content)?
            }
            Ok(())
        }
        fn writelns_indented_iter_display<I, D>(
            f: &mut Formatter<'_>, title: &str, contents: I
        ) -> std::fmt::Result
        where
            I: IntoIterator<Item = D>,
            D: Display
        {
            for content in contents.into_iter() {
                writeln_indented_display(f, title, content)?
            }
            Ok(())
        }
        writelns_indented_iter_str(f, "groups", &pkgbuild.groups)?;
        writelns_indented_iter_str(f, "license", &pkgbuild.license)?;
        let arch_specific = &pkgbuild.multiarch.any;
        writelns_indented_iter_display(f, "checkdepends", &arch_specific.checkdepends)?;
        writelns_indented_iter_display(f, "makedepends", &arch_specific.makedepends)?;
        writelns_indented_iter_display(f, "depends", &arch_specific.depends)?;
        writelns_indented_iter_display(f, "optdepends", &arch_specific.optdepends)?;
        writelns_indented_iter_display(f, "provides", &arch_specific.provides)?;
        writelns_indented_iter_display(f, "conflicts", &arch_specific.conflicts)?;
        writelns_indented_iter_display(f, "replaces", &arch_specific.replaces)?;
        writelns_indented_iter_str(f, "noextract", &pkgbuild.noextract)?;
        macro_rules! write_option {
            ($options: expr, $($option: ident),+) => {
                $(
                    if let Some(value) = $options.$option {
                        writeln!(f, "\toptions = {}{}", 
                            if value {""} else {"!"}, stringify!($option))?
                    }
                )+
            };
            ($options: expr) => {
                write_option!($options, strip, docs, libtool, staticlibs, emptydirs, zipman, 
                    ccache, distcc, buildflags, makeflags, debug, lto);
            };
        }
        write_option!(pkgbuild.options);

        writelns_indented_iter_str(f, "backup", &pkgbuild.backup)?;
        #[derive(Default)]
        struct StatChecksum {
            cksum: bool,
            md5sum: bool,
            sha1sum: bool,
            sha224sum: bool,
            sha256sum: bool,
            sha384sum: bool,
            sha512sum: bool,
            b2sum: bool,
        }
        impl StatChecksum {
            fn ensure_least(&mut self) {
                if !(self.cksum || self.md5sum || self.sha1sum || self.sha224sum
                    || self.sha256sum || self.sha384sum || self.sha512sum ||
                    self.b2sum)
                {
                    self.sha256sum = true
                }
            }
        }
        fn write_sources_and_stat_sums(f: &mut Formatter<'_>, arch_name: &str, arch_specific: &PkgbuildArchSpecific) -> std::result::Result<StatChecksum, std::fmt::Error> {
            let mut stat = StatChecksum::default();
            let title_temp;
            let title = if arch_name.is_empty() {
                "source"
            } else {
                title_temp = format!("source_{}", arch_name);
                &title_temp
            };
            for source_with_checksum in arch_specific.sources_with_checksums.iter() {
                writeln_indented_str(f, title, source_with_checksum.source.get_pkgbuild_source())?;
                macro_rules! update_flag {
                    ($($cksum: ident),+) => {
                        $(
                            if source_with_checksum.$cksum.is_some() { stat.$cksum = true }
                        )+
                    };
                }
                update_flag!(cksum, md5sum, sha1sum, sha224sum, sha256sum, sha384sum, sha512sum, b2sum);
            }
            stat.ensure_least();
            Ok(stat)
        }
        let mut stat_checksums = write_sources_and_stat_sums(f, "", arch_specific)?;
        writelns_indented_iter_str(f, "validpgpkeys", &pkgbuild.validpgpkeys)?;
        fn suffix_from_arch_name(arch_name: &str) -> String {
            if arch_name.is_empty() {
                String::new()
            } else {
                format!("_{}", arch_name)
            }
        }
        fn write_all_checksums(f: &mut Formatter<'_>, stat_checksums: &StatChecksum, 
            arch_name: &str, arch_specific: &PkgbuildArchSpecific
        ) -> std::fmt::Result 
        {
            let suffix = suffix_from_arch_name(arch_name);
            macro_rules! write_checksums {
                ($($cksum: ident),+) => {$(
                    if stat_checksums.$cksum {
                        let title = format!("{}s{}", stringify!($cksum), suffix);
                        for source_with_checksum in arch_specific.sources_with_checksums.iter() {
                            if let Some(bytes) = source_with_checksum.$cksum {
                                write!(f, "\t{} = ", &title)?;
                                write_byte_iter(f, bytes)?;
                                writeln!(f)?
                            } else {
                                writeln_indented_str(f, &title, "SKIP")?
                            }
                        }
                    }
                )+};
            }
            write_checksums!(md5sum, sha1sum, sha224sum, sha256sum, sha384sum, sha512sum, b2sum);
            Ok(())
        }
        write_all_checksums(f, &stat_checksums, "", &arch_specific)?;
        for (arch, arch_specific) in pkgbuild.multiarch.arches.iter() {
            let arch_name = arch.as_ref();
            stat_checksums = write_sources_and_stat_sums(f, arch_name, arch_specific)?;
            writelns_indented_iter_display(f, &format!("provides_{}", arch_name), &arch_specific.provides)?;
            writelns_indented_iter_display(f, &format!("conflicts_{}", arch_name), &arch_specific.conflicts)?;
            writelns_indented_iter_display(f, &format!("depends_{}", arch_name), &arch_specific.depends)?;
            writelns_indented_iter_display(f, &format!("replaces_{}", arch_name), &arch_specific.replaces)?;
            writelns_indented_iter_display(f, &format!("optdepends_{}", arch_name), &arch_specific.optdepends)?;
            writelns_indented_iter_display(f, &format!("makedepends_{}", arch_name), &arch_specific.makedepends)?;
            writelns_indented_iter_display(f, &format!("checkdepends_{}", arch_name), &arch_specific.checkdepends)?;
            write_all_checksums(f, &stat_checksums, arch_name, arch_specific)?
        }
        for pkg in pkgbuild.pkgs.iter() {
            writeln!(f, "\npkgname = {}", pkg.pkgname)?;
            writeln_indented_str(f, "pkgdesc", &pkg.pkgdesc)?;
            writeln_indented_str(f, "url", &pkg.url)?;
            writeln_indented_str(f, "install", &pkg.install)?;
            writeln_indented_str(f, "changelog", &pkg.changelog)?;
            if ! multiarch_have_same_arches(&pkgbuild.multiarch, &pkg.multiarch) {
                if pkg.multiarch.arches.is_empty() {
                    writeln_indented_str(f, "arch", "any")?;
                } else {
                    for (arch, _) in pkg.multiarch.arches.iter() {
                        writeln_indented_str(f, "arch", arch)?;
                    }
                }
            }
            writelns_indented_iter_str(f, "groups", &pkg.groups)?;
            writelns_indented_iter_str(f, "license", &pkg.license)?;
            let arch_specific = &pkg.multiarch.any;
            writelns_indented_iter_display(f, "checkdepends", &arch_specific.checkdepends)?;
            writelns_indented_iter_display(f, "depends", &arch_specific.depends)?;
            writelns_indented_iter_display(f, "optdepends", &arch_specific.optdepends)?;
            writelns_indented_iter_display(f, "provides", &arch_specific.provides)?;
            writelns_indented_iter_display(f, "conflicts", &arch_specific.conflicts)?;
            writelns_indented_iter_display(f, "replaces", &arch_specific.replaces)?;
            write_option!(pkg.options);
            writelns_indented_iter_str(f, "backup", &pkg.backup)?;
            for (arch, arch_specific) in pkg.multiarch.arches.iter() {
                let arch_name = arch.as_ref();
                writelns_indented_iter_display(f, &format!("provides_{}", arch_name), &arch_specific.provides)?;
                writelns_indented_iter_display(f, &format!("conflicts_{}", arch_name), &arch_specific.conflicts)?;
                writelns_indented_iter_display(f, &format!("depends_{}", arch_name), &arch_specific.depends)?;
                writelns_indented_iter_display(f, &format!("replaces_{}", arch_name), &arch_specific.replaces)?;
                writelns_indented_iter_display(f, &format!("optdepends_{}", arch_name), &arch_specific.optdepends)?;
                writelns_indented_iter_display(f, &format!("checkdepends_{}", arch_name), &arch_specific.checkdepends)?;
            }
        }
        Ok(())
    }
}
