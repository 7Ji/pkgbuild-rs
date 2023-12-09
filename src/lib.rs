use std::{ffi::{OsString, OsStr}, path::{PathBuf, Path}, os::unix::ffi::OsStrExt, io::{Write, BufWriter, Read}, process::{Command, Stdio, Child, ChildStdin, ChildStdout, ChildStderr}, fmt::{Display, Formatter}};

use hex::FromHex;
use tempfile::NamedTempFile;
#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};
#[cfg(feature = "serde")]
mod serde_optional_bytes_arrays;
#[cfg(feature = "nothread")]
use libc::{PIPE_BUF, EAGAIN};
#[cfg(feature = "nothread")]
use nix::fcntl::{fcntl, FcntlArg::F_SETFL, OFlag};
#[cfg(feature = "nothread")]
use std::os::fd::AsRawFd;
#[cfg(not(feature = "nothread"))]
use std::thread::spawn;

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
            Error::ThreadUnjoinable => write!(f, "Thread Not Joinable"),
            Error::BrokenPKGBUILDs(e) => 
                write!(f, "PKGBUILDs Broken ({})", e.len()),
            Error::ParserScriptIllegalOutput(e) => write!(
                f, "Parser Script Illegal Output: {}", str_from_slice_u8!(e)),
        }
    }
}

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

    /// When dumping arrays like `depends` and `makedepends`, also dump the
    /// values from the corresponding arch-specific array `depends_${CARCH}`
    /// 
    /// Note that different from `makepkg --printsrcinfo`, these values would
    /// be included in the corresponding generic array and not considered arch-
    /// specific anymore. 
    /// 
    /// I.e., with `CARCH=x86_64`, the following two `PKGBUILD`s yeild the same
    /// `source` array:
    /// 
    /// `PKGBUILD`1:
    /// ```Bash
    /// source=('file1' 'file2')
    /// source_x86_64=('file3')
    /// ```
    /// `PKGBUILD`2:
    /// ```Bash
    /// source=('file1' 'file2' 'file3')
    /// ```
    /// This is by-design as a repo builder should always handle all of its
    /// native arch-specific vars as if they're generic. And to create seperate
    /// arrays for each `CARCH` is simply impossible for a strongly-typed Rust-
    /// native `Pkgbuild` structure, which could be freely set be a user, 
    /// regardless of whether that's even an actual existing `CARCH`
    /// 
    /// The value `CARCH` should be set in `makepkg_config`
    /// 
    /// Default: `true`
    pub arch_specific: bool,

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
            arch_specific: true,
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

    /// Set whether the `pkgbase` should be dumped from `PKGBUILD`
    pub fn set_pkgbase(&mut self, pkgbase: bool) -> &mut Self {
        self.pkgbase = pkgbase;
        self
    }

    /// Set whether the `pkgname` array should be dumped from `PKGBUILD`.
    /// 
    /// Disabling this would also cause every `PKGBUILD` to be treated as if 
    /// it is not a split-package `PKGBUILD`
    pub fn set_pkgname(&mut self, pkgname: bool) -> &mut Self {
        self.pkgname = pkgname;
        self
    }

    /// Set whether the `pkgver` should be dumped from `PKGBUILD`
    /// 
    /// If disabled, the result field `version` would always have an empty 
    /// `pkgver` field
    pub fn set_pkgver(&mut self, pkgver: bool) -> &mut Self {
        self.pkgver = pkgver;
        self
    }

    /// Set whether the `pkgrel` should be dumped from `PKGBUILD`
    /// 
    /// If disabled, the result field `version` would always have an empty 
    /// `pkgrel` field
    pub fn set_pkgrel(&mut self, pkgrel: bool) -> &mut Self {
        self.pkgrel = pkgrel;
        self
    }

    /// Set whether the `epoch` should be dumped from `PKGBUILD`
    /// 
    /// If disabled, the result field `version` would always have an empty 
    /// `epoch` field
    pub fn set_epoch(&mut self, epoch: bool) -> &mut Self {
        self.epoch = epoch;
        self
    }

    /// Set whether the arch-specific array should be appended to the generic
    /// array when dumping `source`, `depends`, etc
    /// 
    /// The arch should be set as `CARCH` in the `makepkg_conf`
    /// 
    /// If disabled, the parsing result is as if we're parsing on an unkown
    /// architecture.
    pub fn set_arch_specific(&mut self, arch_specific: bool) -> &mut Self {
        self.arch_specific = arch_specific;
        self
    }

    /// Set whether the `depends` array should be dumped
    pub fn set_depends(&mut self, depends: bool) -> &mut Self {
        self.depends = depends;
        self
    }

    /// Set whether the `makedepends` array should be dumped
    pub fn set_makedepends(&mut self, makedepends: bool) -> &mut Self {
        self.makedepends = makedepends;
        self
    }

    /// Set whether the `provides` array should be dumped
    pub fn set_provides(&mut self, provides: bool) -> &mut Self {
        self.provides = provides;
        self
    }

    /// Set whether the `cksums` array should be dumped
    pub fn set_cksums(&mut self, cksums: bool) -> &mut Self {
        self.cksums = cksums;
        self
    }

    pub fn set_md5sums(&mut self, md5sums: bool) -> &mut Self {
        self.md5sums = md5sums;
        self
    }

    pub fn set_sha1sums(&mut self, sha1sums: bool) -> &mut Self {
        self.sha1sums = sha1sums;
        self
    }

    pub fn set_sha224sums(&mut self, sha224sums: bool) -> &mut Self {
        self.sha224sums = sha224sums;
        self
    }

    pub fn set_sha256sums(&mut self, sha256sums: bool) -> &mut Self {
        self.sha256sums = sha256sums;
        self
    }

    pub fn set_sha384sums(&mut self, sha384sums: bool) -> &mut Self {
        self.sha384sums = sha384sums;
        self
    }

    pub fn set_sha512sums(&mut self, sha512sums: bool) -> &mut Self {
        self.sha512sums = sha512sums;
        self
    }

    pub fn set_b2sums(&mut self, b2sums: bool) -> &mut Self {
        self.b2sums = b2sums;
        self
    }

    pub fn set_pkgver_func(&mut self, pkgver_func: bool) -> &mut Self {
        self.pkgver_func = pkgver_func;
        self
    }

    /// Set whether the package-specific depends array should be dumped
    pub fn set_package_depends(&mut self, package_depends: bool) -> &mut Self {
        self.package_depends = package_depends;
        self
    }

    /// Set whether the package-specific makedepends array should be dumped
    pub fn set_package_makedepends(&mut self, package_makedepends: bool) 
        -> &mut Self 
    {
        self.package_makedepends = package_makedepends;
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
        writer.write_all(include_bytes!(
            "script/10_source_lib_config.bash"))?;
        let func_dump_array: &[u8] = 
            if self.arch_specific {
                writer.write_all(include_bytes!(
                    "script/21_func_dump_array_with_optional_arch.bash"))?;
                b"dump_array_with_optional_arch"
            } else {
                writer.write_all(include_bytes!(
                    "script/20_func_dump_array.bash"))?;
                b"dump_array"
            };
        if self.package_depends || self.package_makedepends {
            writer.write_all(include_bytes!(
                "script/22_func_extract_package_vars.bash"))?;
        }
        writer.write_all(include_bytes!("script/30_loop_start.bash"))?;
        if self.pkgbase {
            writer.write_all(b"echo \"base:${pkgbase}\"\n")?;
        }
        if self.pkgname {
            writer.write_all(b"for item in \"${pkgname[@]}\"; do \
                                    echo \"name:${item}\"; done\n")?
        } else {
            writer.write_all(b"for item in \"${pkgname[@]}\"; do \
                                        unset -f package_\"${item}\"; \
                                    done\n\
                                    pkgname=\"${pkgbase}\"\n")?
        }
        if self.pkgver {
            writer.write_all(b"echo \"ver:${pkgver}\"\n")?
        }
        if self.pkgrel {
            writer.write_all(b"echo \"rel:${pkgrel}\"\n")?
        }
        if self.epoch {
            writer.write_all(b"echo \"epoch:${epoch}\"\n")?
        }
        if self.depends {
            writer.write_all(func_dump_array)?;
            writer.write_all(b" depends dep\n")?
        }
        if self.makedepends {
            writer.write_all(func_dump_array)?;
            writer.write_all(b" makedepends makedep\n")?
        }
        if self.provides {
            writer.write_all(func_dump_array)?;
            writer.write_all(b" provides provide\n")?
        }
        if self.source {
            writer.write_all(func_dump_array)?;
            writer.write_all(b" source source\n")?
        }
        if self.cksums {
            writer.write_all(func_dump_array)?;
            writer.write_all(b" cksums ck\n")?
        }
        if self.md5sums {
            writer.write_all(func_dump_array)?;
            writer.write_all(b" md5sums md5\n")?
        }
        if self.sha1sums {
            writer.write_all(func_dump_array)?;
            writer.write_all(b" sha1sums sha1\n")?
        }
        if self.sha224sums {
            writer.write_all(func_dump_array)?;
            writer.write_all(b" sha224sums sha224\n")?
        }
        if self.sha256sums {
            writer.write_all(func_dump_array)?;
            writer.write_all(b" sha256sums sha256\n")?
        }
        if self.sha384sums {
            writer.write_all(func_dump_array)?;
            writer.write_all(b" sha384sums sha384\n")?
        }
        if self.sha512sums {
            writer.write_all(func_dump_array)?;
            writer.write_all(b" sha512sums sha512\n")?
        }
        if self.b2sums {
            writer.write_all(func_dump_array)?;
            writer.write_all(b" b2sums b2\n")?
        }
        if self.pkgver_func {
            writer.write_all(b"echo -n 'pkgver_func:'\n")?;
            writer.write_all(b"[[ $(type -t pkgver) == 'function' ]] && \
                                echo y || echo n\n")?
        }
        if self.pkgname && (self.package_depends || self.package_makedepends) {
            writer.write_all(
                b"unset -v {depends,provides}{,_\"${CARCH}\"}\n\
                extract_package_vars\n")?;
            if self.package_depends {
                writer.write_all(func_dump_array)?;
                writer.write_all(b" depends dep_\"${pkgbase}\"\n")?
            }
            if self.package_makedepends {
                writer.write_all(func_dump_array)?;
                writer.write_all(b" provides provide_\"${pkgbase}\"\n")?
            }
            writer.write_all(b"for item in \"${pkgname[@]}\"; do\n\
                unset -v {depends,provides}{,_\"${CARCH}\"}\n\
                extract_package_vars _\"${item}\"\n")?;
            if self.package_depends {
                writer.write_all(func_dump_array)?;
                writer.write_all(b" depends dep_\"${item}\"\n")?
            }
            if self.package_makedepends {
                writer.write_all(func_dump_array)?;
                writer.write_all(b" provides provide_\"${item}\"\n")?
            }
            writer.write_all(b"done\n")?
        }
        writer.write_all(b"unset -v pkgbase pkgname pkgver pkgrel epoch \
            {depends,makedepends,provides,source,\
            {ck,md5,sha{1,224,256,384,512},b2}sums}{,_\"${CARCH}\"}\n\
            unset -f pkgver package\n\
            done\n")
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
            let mut end = written + libc::PIPE_BUF;
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
            .arg("-e")
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
                if let Err(e) = child.wait() {
                    log::error!("Failed to wait for killed child: {}", e);
                    return Err(e.into())
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
#[derive(Default)]
struct PackageParsing<'a> {
    pkgname: &'a [u8],
    depends: Vec<&'a [u8]>,
    provides: Vec<&'a [u8]>,
}

/// A `PKGBUILD` being parsed. Library users should
/// not use this.
#[derive(Default)]
struct PkgbuildParsing<'a> {
    pkgbase: &'a [u8],
    pkgs: Vec<PackageParsing<'a>>,
    pkgver: &'a [u8],
    pkgrel: &'a [u8],
    epoch: &'a [u8],
    depends: Vec<&'a [u8]>,
    makedepends: Vec<&'a [u8]>,
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

#[derive(Default)]
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
                            '{}'", str_from_slice_u8!(line));
                        return Err(Error::BrokenPKGBUILDs(Vec::new()));
                    },
                };
                let value = match it.next() {
                    Some(value) => value,
                    None => {
                        log::error!("Failed to get value from PKGBUILD, at line\
                            : '{}'", str_from_slice_u8!(line));
                        return Err(Error::BrokenPKGBUILDs(Vec::new()));
                    },
                };
                if it.next().is_some() {
                    log::error!("PKGBUILD parsing line not sustained: '{}'", 
                    str_from_slice_u8!(line));
                    return Err(Error::ParserScriptIllegalOutput(line.into()));
                }
                match key {
                    b"base" => pkgbuild.pkgbase = value,
                    b"name" => {
                        let mut pkg =
                            PackageParsing::default();
                        pkg.pkgname = value;
                        pkgbuild.pkgs.push(pkg);
                    },
                    b"ver" => pkgbuild.pkgver = value,
                    b"rel" => pkgbuild.pkgrel = value,
                    b"epoch" => pkgbuild.epoch = value,
                    b"dep" => pkgbuild.depends.push(value),
                    b"makedep" => pkgbuild.makedepends.push(value),
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
                                str_from_slice_u8!(value));
                            return Err(Error::BrokenPKGBUILDs(Vec::new()))
                        }
                    }
                    _ => {
                        let (offset, is_dep) =
                        if key.starts_with(b"dep_") {(4, true)}
                        else if key.starts_with(b"provide_") {(8, false)}
                        else {
                            log::error!("Unexpected line: {}",
                                str_from_slice_u8!(line));
                            return Err(Error::BrokenPKGBUILDs(Vec::new()))
                        };
                        let name = &key[offset..];
                        let mut pkg = None;
                        for pkg_cmp in
                            pkgbuild.pkgs.iter_mut()
                        {
                            if pkg_cmp.pkgname == name {
                                pkg = Some(pkg_cmp);
                                break
                            }
                        }
                        let pkg = match pkg {
                            Some(pkg) => pkg,
                            None => {
                                log::error!("Failed to find pkg {}",
                                    str_from_slice_u8!(name));
                                return Err(Error::BrokenPKGBUILDs(Vec::new()))
                            },
                        };
                        if is_dep {
                            pkg.depends.push(value)
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
                    started = true;
                }
            } else {
                log::error!("Illegal line: {}", str_from_slice_u8!(line));
                return Err(Error::BrokenPKGBUILDs(Vec::new()))
            }
        }
        if started {
            pkgbuilds.push(pkgbuild)
        }
        Ok(Self {
            entries: pkgbuilds,
        })
    }
}

#[derive(Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct UnorderedVersion {
    pub epoch: String,
    pub pkgver: String,
    pub pkgrel: String
}

#[cfg(feature = "format")]
impl Display for UnorderedVersion {
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

impl TryFrom<&str> for UnorderedVersion {
    type Error = Error;
    
    fn try_from(value: &str) -> Result<Self> {
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
        Ok(Self { epoch, pkgver, pkgrel })
    }
}

impl TryFrom<&[u8]> for UnorderedVersion {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self> {
        Self::try_from(str_from_slice_u8!(value))
    }
}

impl UnorderedVersion {
    fn from_raw(epoch: &[u8], pkgver: &[u8], pkgrel: &[u8]) -> Self {
        Self {
            epoch: string_from_slice_u8!(epoch),
            pkgver: string_from_slice_u8!(pkgver),
            pkgrel: string_from_slice_u8!(pkgrel),
        }
    }
}

/// The dependency order, comparision is not implemented yet
#[derive(Debug, PartialEq)]
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
#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OrderedVersion {
    pub order: DependencyOrder,
    /// The version info without ordering
    pub unordered: UnorderedVersion,
}

#[cfg(feature = "format")]
impl Display for OrderedVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.order, self.unordered)
    }
}


/// A dependency
#[derive(Debug, PartialEq)]
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

impl TryFrom<&str> for Dependency {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        if let Some((name, version)) = 
            value.split_once("=") 
        {
            if let Some((name, version)) = 
                value.split_once(">=") 
            {
                Ok(Self { name: name.into(), 
                    version: Some(OrderedVersion { 
                        order: DependencyOrder::GreaterOrEqual, 
                        unordered: version.try_into()? }) })
            } else if let Some((name, version)) = 
                value.split_once("<=") 
            {
                Ok(Self { name: name.into(), 
                    version: Some(OrderedVersion { 
                        order: DependencyOrder::LessOrEqual, 
                        unordered: version.try_into()? }) })
            } else {
                Ok(Self { name: name.into(), 
                    version: Some(OrderedVersion { 
                        order: DependencyOrder::Equal, 
                        unordered: version.try_into()? }) })
            }
        } else if let Some((name, version)) = 
            value.split_once('>') 
        {
            Ok(Self { name: name.into(), 
                version: Some(OrderedVersion { 
                    order: DependencyOrder::Greater, 
                    unordered: version.try_into()? }) })

        } else if let Some((name, version)) = 
            value.split_once('<') 
        {
            Ok(Self { name: name.into(), 
                version: Some(OrderedVersion { 
                    order: DependencyOrder::Less, 
                    unordered: version.try_into()? }) })
        } else {
            Ok(Self {name: value.into(), version: None})
        }
    }
}

impl TryFrom<&[u8]> for Dependency {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self> {
        Self::try_from(str_from_slice_u8!(value))
    }
}

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Provide {
    pub name: String,
    pub version: Option<UnorderedVersion>
}

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
                version: Some(version.try_into()?) }) 
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

/// A sub-package parsed from a split-package `PKGBUILD`
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Package {
    /// The name of the split pacakge
    pub pkgname: String,
    /// The dependencies of the split package
    pub depends: Vec<Dependency>,
    /// What virtual packages does this package provide
    pub provides: Vec<Provide>,
}

#[cfg(feature = "format")]
fn format_write_array<I, D>(f: &mut Formatter<'_>, array: I) 
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
        format_write_array(f, &self.depends)?;
        write!(f, "], provides: [")?;
        format_write_array(f, &self.provides)?;
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
        match self.protocol {
            SourceProtocol::Unknown | SourceProtocol::Local => (),
            _ =>
                if proto_actual != proto_url {
                    raw.push_str(proto_actual);
                    raw.push('+');
                }
        }
        raw.push_str(&self.url);
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

/// A source with its integrity checksum. Do note that each source could have
/// multiple integrity checksums defined. For efficiency this is not directly
/// returned in the `Pkgbuild`. 
pub struct SourceWithInteg {
    pub source: Source,
    pub cksum: Option<Cksum>,
    pub md5sum: Option<Md5sum>,
    pub sha1sum: Option<Sha1sum>,
    pub sha224sum: Option<Sha224sum>,
    pub sha256sum: Option<Sha256sum>,
    pub sha384sum: Option<Sha384sum>,
    pub sha512sum: Option<Sha512sum>,
    pub b2sum: Option<B2sum>
}

impl From<&Source> for SourceWithInteg {
    fn from(value: &Source) -> Self {
        Self {
            source: value.clone(),
            cksum: None,
            md5sum: None,
            sha1sum: None,
            sha224sum: None,
            sha256sum: None,
            sha384sum: None,
            sha512sum: None,
            b2sum: None,
        }
    }
}

/// A `PKGBUILD` that could potentially have multiple split-packages
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Pkgbuild {
    pub pkgbase: String,
    pub pkgs: Vec<Package>,
    pub version: UnorderedVersion,
    pub depends: Vec<Dependency>,
    pub makedepends: Vec<Dependency>,
    pub provides: Vec<Provide>,
    pub sources: Vec<Source>,
    pub cksums: Vec<Option<Cksum>>,
    #[cfg_attr(feature = "serde", serde(with = "serde_optional_bytes_arrays"))]
    pub md5sums: Vec<Option<Md5sum>>,
    #[cfg_attr(feature = "serde", serde(with = "serde_optional_bytes_arrays"))]
    pub sha1sums: Vec<Option<Sha1sum>>,
    #[cfg_attr(feature = "serde", serde(with = "serde_optional_bytes_arrays"))]
    pub sha224sums: Vec<Option<Sha224sum>>,
    #[cfg_attr(feature = "serde", serde(with = "serde_optional_bytes_arrays"))]
    pub sha256sums: Vec<Option<Sha256sum>>,
    #[cfg_attr(feature = "serde", serde(with = "serde_optional_bytes_arrays"))]
    pub sha384sums: Vec<Option<Sha384sum>>,
    #[cfg_attr(feature = "serde", serde(with = "serde_optional_bytes_arrays"))]
    pub sha512sums: Vec<Option<Sha512sum>>,
    #[cfg_attr(feature = "serde", serde(with = "serde_optional_bytes_arrays"))]
    pub b2sums: Vec<Option<B2sum>>,
    pub pkgver_func: bool,
}

#[cfg(feature = "format")]
fn format_write_cksum_array<'a, I>(f: &mut Formatter<'_>, array: I) 
-> std::fmt::Result 
where
    I: IntoIterator<Item = &'a Option<Cksum>>
{
    let mut start = false;
    for item in array.into_iter() {
        if start {
            write!(f, ", ")?
        } else {
            start = true
        }
        if let Some(cksum) = item {
            write!(f, "{:08x}", cksum)?
        } else {
            write!(f, "SKIP")?
        }
    }
    Ok(())
}

#[cfg(feature = "format")]
fn format_write_integ_sums_array<'a, I, S>(f: &mut Formatter<'_>, array: I) 
-> std::fmt::Result 
where
    I: IntoIterator<Item = &'a Option<S>>,
    S: AsRef<[u8]> + 'a
{

    let mut start = false;
    for item in array.into_iter() {
        if start {
            write!(f, ", ")?
        } else {
            start = true
        }
        if let Some(cksum) = item {
            for byte in cksum.as_ref().iter() {
                write!(f, "{:02x}", byte)?
            }
        } else {
            write!(f, "SKIP")?
        }
    }
    Ok(())
}

#[cfg(feature = "format")]
impl Display for Pkgbuild {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{base: {}, pkgs: [", self.pkgbase)?;
        format_write_array(f, &self.pkgs)?;
        write!(f, "], version: {}", self.version)?;
        if self.pkgver_func {
            write!(f, " (has pkgver func)")?;
        }
        write!(f, ", depends: [")?;
        format_write_array(f, &self.depends)?;
        write!(f, "], makedepends: [")?;
        format_write_array(f, &self.makedepends)?;
        write!(f, "], provides: [")?;
        format_write_array(f, &self.provides)?;
        write!(f, "], sources: [")?;
        format_write_array(f, &self.sources)?;
        write!(f, "], cksums: [")?;
        format_write_cksum_array(f, &self.cksums)?;
        write!(f, "], md5sums: [")?;
        format_write_integ_sums_array(f, &self.md5sums)?;
        write!(f, "], sha1sums: [")?;
        format_write_integ_sums_array(f, &self.sha1sums)?;
        write!(f, "], sha224sums: [")?;
        format_write_integ_sums_array(f, &self.sha224sums)?;
        write!(f, "], sha256sums: [")?;
        format_write_integ_sums_array(f, &self.sha256sums)?;
        write!(f, "], sha384sums: [")?;
        format_write_integ_sums_array(f, &self.sha384sums)?;
        write!(f, "], sha512sums: [")?;
        format_write_integ_sums_array(f, &self.sha512sums)?;
        write!(f, "], b2sums: [")?;
        format_write_integ_sums_array(f, &self.b2sums)?;
        write!(f, "]}}")
    }
}

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

impl TryFrom<&PackageParsing<'_>> for Package {
    type Error = Error;

    fn try_from(value: &PackageParsing) -> Result<Self> {
        let mut depends = Vec::new();
        for depend in value.depends.iter() {
            depends.push(str_from_slice_u8!(depend).try_into()?)
        }
        let mut provides = Vec::new();
        for provide in value.provides.iter() {
            provides.push(str_from_slice_u8!(provide).try_into()?)
        }
        let pkgname = string_from_slice_u8!(value.pkgname);
        Ok(Self { pkgname, depends, provides })
    }
}

fn cksum_from_raw(raw: &&[u8]) -> Option<u32> {
    if raw == b"SKIP" {
        return None
    }
    #[cfg(feature = "unsafe_str")]
    return unsafe{std::str::from_utf8_unchecked(raw)}.parse().ok();
    #[cfg(not(feature = "unsafe_str"))]
    String::from_utf8_lossy(raw).parse().ok()
}

fn cksums_from_raws<'a, I>(raws: I) -> Vec<Option<u32>> 
where
    I: IntoIterator<Item = &'a &'a [u8]>
{
    raws.into_iter().map(|raw|cksum_from_raw(raw)).collect()
}

fn hash_sum_from_hex<C: FromHex>(hex: &&[u8]) -> Option<C> {
    if hex == b"SKIP" {
        return None
    }
    FromHex::from_hex(hex).ok()
}

fn hash_sums_from_hexes<'a, I, C>(hexes: I) -> Vec<Option<C>> 
where
    I: IntoIterator<Item = &'a &'a [u8]>,
    C: FromHex
{
    hexes.into_iter().map(|hex|hash_sum_from_hex(hex)).collect()
}

impl TryFrom<&PkgbuildParsing<'_>> for Pkgbuild {
    type Error = Error;

    fn try_from(value: &PkgbuildParsing) -> Result<Self> {
        let mut pkgs = Vec::new();
        for pkg in value.pkgs.iter() {
            pkgs.push(pkg.try_into()?)
        }
        let mut depends = Vec::new();
        for depend in value.depends.iter() {
            depends.push((*depend).try_into()?)
        }
        let mut makedepends = Vec::new();
        for makedepend in value.makedepends.iter() {
            makedepends.push((*makedepend).try_into()?)
        }
        let mut provides = Vec::new();
        for provide in value.provides.iter() {
            provides.push((*provide).try_into()?)
        }
        Ok(Self {
            pkgbase: string_from_slice_u8!(value.pkgbase),
            pkgs,
            version: UnorderedVersion::from_raw(
                value.epoch, value.pkgver, value.pkgrel),
            depends,
            makedepends,
            provides,
            sources: value.sources.iter().map(|source|
                (*source).into()).collect(),
            cksums: cksums_from_raws(&value.cksums),
            md5sums: hash_sums_from_hexes(&value.md5sums),
            sha1sums: hash_sums_from_hexes(&value.sha1sums),
            sha224sums: hash_sums_from_hexes(&value.sha224sums),
            sha256sums: hash_sums_from_hexes(&value.sha256sums),
            sha384sums: hash_sums_from_hexes(&value.sha384sums),
            sha512sums: hash_sums_from_hexes(&value.sha512sums),
            b2sums: hash_sums_from_hexes(&value.b2sums),
            pkgver_func: value.pkgver_func,
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
    /// Get sources with the integrity checksums attached, i.e. get a `Vec` of 
    /// `SourceWithInteg`, with each of them containing a cloned `Source`, and 
    /// cloned integrity checksums.
    /// 
    /// This is useful if you want to download and prepare the sources in `Rust`
    /// world directly without relying on `makepkg`'s internal implementation.
    /// 
    /// Note however, the result would always take the full space for all 
    /// variants of checksums, even if you've disabled the parsing in the
    /// `ParserScript`
    pub fn get_sources_with_integ(&self) -> Vec<SourceWithInteg> {
        let mut sources: Vec<SourceWithInteg> = Vec::new();
        for source in self.sources.iter() {
            sources.push(source.into())
        }
        for (source, cksum) in 
            sources.iter_mut().zip(self.cksums.iter()) 
        {
            source.cksum = *cksum;
        }
        for (source, md5sum) in 
            sources.iter_mut().zip(self.md5sums.iter()) 
        {
            source.md5sum = *md5sum;
        }
        for (source, sha1sum) in 
            sources.iter_mut().zip(self.sha1sums.iter()) 
        {
            source.sha1sum = *sha1sum;
        }
        for (source, sha224sum) in 
            sources.iter_mut().zip(self.sha224sums.iter()) 
        {
            source.sha224sum = *sha224sum;
        }
        for (source, sha256sum) in 
            sources.iter_mut().zip(self.sha256sums.iter()) 
        {
            source.sha256sum = *sha256sum;
        }
        for (source, sha384sum) in 
            sources.iter_mut().zip(self.sha384sums.iter()) 
        {
            source.sha384sum = *sha384sum;
        }
        for (source, sha512sum) in 
            sources.iter_mut().zip(self.sha512sums.iter()) 
        {
            source.sha512sum = *sha512sum;
        }
        for (source, b2sum) in 
            sources.iter_mut().zip(self.b2sums.iter()) 
        {
            source.b2sum = *b2sum;
        }
        sources
    }
}

impl Pkgbuilds {
    /// Get each `Pkgbuild`'s sources with the integrity checksums attached, 
    /// i.e. get a `Vec` of `Vec` of `SourceWithInteg`, with each of them 
    /// containing a cloned `Source`, and cloned integrity checksums.
    /// 
    /// This is a shortcut that calls `get_sources_with_integ()` for each entry
    /// `Pkgbuild`s and collects the results into a `Vec`.
    /// 
    /// This is useful if you want to download and prepare the sources in `Rust`
    /// world directly without relying on `makepkg`'s internal implementation.
    /// 
    /// Note however, the result would always take the full space for all 
    /// variants of checksums, even if you've disabled the parsing in the
    /// `ParserScript`
    pub fn get_each_sources_with_integ(&self) -> Vec<Vec<SourceWithInteg>> {
        self.entries.iter().map(|pkgbuild|
            pkgbuild.get_sources_with_integ()).collect()
    }

    /// Get all `Pkgbuild`'s sources with the integrity checksums attached,
    /// i.e. get a `Vec` of `SourceWithInteg`, with each of them 
    /// containing a cloned `Source`, and cloned integrity checksums.
    /// 
    /// This is a shortcut that calls `get_sources_with_integ()` for each entry
    /// `Pkgbuild`s and take out all of the results to collect them into a giant
    /// single `Vec`.
    /// 
    /// This is useful if you want to download and prepare the sources in `Rust`
    /// world directly without relying on `makepkg`'s internal implementation.
    /// 
    /// Note however, the result would always take the full space for all 
    /// variants of checksums, even if you've disabled the parsing in the
    /// `ParserScript`
    pub fn get_all_sources_with_integ(&self) -> Vec<SourceWithInteg> {
        let mut sources = Vec::new();
        for pkgbuild in self.entries.iter() {
            let mut sources_this = 
                pkgbuild.get_sources_with_integ();
            sources.append(&mut sources_this);
        }
        sources
    }
}