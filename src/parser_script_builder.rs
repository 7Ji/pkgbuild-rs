use std::{ffi::{OsString, OsStr}, io::{BufWriter, Write}, path::Path, os::unix::ffi::OsStrExt};

use crate::{error::{Error,Result}, ParserScript};


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
