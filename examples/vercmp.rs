use pkgbuild::UnorderedVersion;

fn main() {
    let mut args = std::env::args();
    let arg_ver1 = args.nth(1).unwrap();
    let arg_ver2 = args.nth(0).unwrap();
    let ver1 = UnorderedVersion::try_from(arg_ver1.as_str()).unwrap();
    let ver2 = UnorderedVersion::try_from(arg_ver2.as_str()).unwrap();
    println!("Comparing version '{}' as '{:?}' and version '{}' as '{:?}': {:?}", 
        arg_ver1, ver1, arg_ver2, ver2, ver1.cmp(&ver2));
}