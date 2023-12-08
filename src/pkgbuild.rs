use hex::FromHex;
#[cfg(feature = "format")]
use std::fmt::{Display, Formatter};
#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::error::{Error, Result};
use crate::serde_optional_bytes_arrays;

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
pub(crate) struct PkgbuildsParsing<'a> {
    entries: Vec<PkgbuildParsing<'a>>
}

impl<'a> PkgbuildsParsing<'a> {
    pub(crate) fn from_parser_output(output: &'a Vec<u8>) -> Result<Self> {
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
                            if pkg_cmp.pkgname == name {
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
        Self::try_from(String::from_utf8_lossy(value).as_ref())
    }
}

fn string_from_slice_u8(original: &[u8]) -> String {
    String::from_utf8_lossy(original).into()
}

impl UnorderedVersion {
    fn from_raw(epoch: &[u8], pkgver: &[u8], pkgrel: &[u8]) -> Self {
        Self {
            epoch: string_from_slice_u8(epoch),
            pkgver: string_from_slice_u8(pkgver),
            pkgrel: string_from_slice_u8(pkgrel),
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
        Self::try_from(String::from_utf8_lossy(value).as_ref())
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
        Self::try_from(String::from_utf8_lossy(value).as_ref())
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
        String::from_utf8_lossy(value).as_ref().into()
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
    pub entries: Vec<Pkgbuild>
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
            depends.push(String::from_utf8_lossy(depend).as_ref().try_into()?)
        }
        let mut provides = Vec::new();
        for provide in value.provides.iter() {
            provides.push(String::from_utf8_lossy(provide).as_ref().try_into()?)
        }
        let pkgname = String::from_utf8_lossy(value.pkgname).into();
        Ok(Self { pkgname, depends, provides })
    }
}

fn cksum_from_raw(raw: &&[u8]) -> Option<u32> {
    if raw == b"SKIP" {
        return None
    }
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
            pkgbase: string_from_slice_u8(value.pkgbase),
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