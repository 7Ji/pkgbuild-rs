use std::io::Write;

fn main() {
    let mut file = std::fs::File::create("src/script/full.bash")
        .expect("Failed to create full script at 'src/script/full.bash'");
    fn buffer_extend_indent(
        buffer: &mut Vec<u8>, indent_level: usize
    ) {
        for _ in 0..indent_level {
            buffer.extend_from_slice(b"  ")
        }
    }
    fn buffer_extend_dump_plain(
        buffer: &mut Vec<u8>, name: &[u8], indent_level: usize
    ) {
        buffer_extend_indent(buffer, indent_level);
        buffer.extend_from_slice(b"echo ");
        buffer.extend_from_slice(name);
        buffer.extend_from_slice(b":\"${");
        buffer.extend_from_slice(name);
        buffer.extend_from_slice(b"}\"\n");
    }
    fn buffer_extend_multi_dump_plain(
        buffer: &mut Vec<u8>, names: &[&[u8]], indent_level: usize
    ) {
        names.iter().for_each(|name|
            buffer_extend_dump_plain(buffer, name, indent_level))
    }
    fn buffer_extend_dump_array_license_workaround(
        buffer: &mut Vec<u8>, indent_level: usize
    ) {
        buffer_extend_indent(buffer, indent_level);
        buffer.extend_from_slice(b"license=(\"${license[@]//\n/ }\")\n");
    }
    fn buffer_extend_dump_array(
        buffer: &mut Vec<u8>, name: &[u8], indent_level: usize
    ) {
        buffer_extend_indent(buffer, indent_level);
        buffer.extend_from_slice(b"printf '");
        buffer.extend_from_slice(name);
        buffer.extend_from_slice(b":%s\\n' \"${");
        buffer.extend_from_slice(name);
        buffer.extend_from_slice(b"[@]}\"\n");
    }
    fn buffer_extend_multi_dump_array(
        buffer: &mut Vec<u8>, names: &[&[u8]], indent_level: usize
    ) {
        names.iter().for_each(|name|
            buffer_extend_dump_array(buffer, name, indent_level))
    }
    fn buffer_extend_dump_arch_array(
        buffer: &mut Vec<u8>, names: &[&[u8]], indent_level: usize
    ) {
        buffer_extend_indent(buffer, indent_level);
        buffer.extend_from_slice(b"declare -n");
        for items in names.iter() {
            buffer.extend_from_slice(b" _arch_");
            buffer.extend_from_slice(items);
            buffer.push(b'=');
            buffer.extend_from_slice(items);
            buffer.extend_from_slice(b"_\"${_arch}\"");
        }
        buffer.push(b'\n');
        for items in names.iter() {
            buffer_extend_indent(buffer, indent_level);
            buffer.extend_from_slice(b"printf '");
            buffer.extend_from_slice(items);
            buffer.extend_from_slice(b":%s\\n' \"${_arch_");
            buffer.extend_from_slice(items);
            buffer.extend_from_slice(b"[@]}\"\n");
        }
    }
    fn buffer_extend_multicase(buffer: &mut Vec<u8>, names: &[&[u8]]) {
        let mut started = false;
        for name in names.iter() {
            if started {
                buffer.push(b'|')
            } else {
                started = true
            }
            buffer.extend_from_slice(name)
        }
    }
    // Try to expand as many loops as possible
    const PKGBUILD_PLAIN_ITEMS: &[&[u8]] = &[
        b"pkgbase", b"pkgver", b"pkgrel", b"epoch", b"pkgdesc",
        b"url", b"install", b"changelog"];
    const PKGBUILD_ARRAY_ITEMS: &[&[u8]] = &[
        b"license", b"validpgpkeys", b"noextract", 
        b"groups", b"backup", b"options"];
    const PACKAGE_PLAIN_ITEMS: &[&[u8]] = &[
        b"pkgdesc", b"url", b"install", b"changelog"];
    const PACKAGE_ARRAY_ITEMS: &[&[u8]] = &[
        b"license", b"groups", b"backup", b"options"];
    const PKGBUILD_ARCH_SPECIFIC_ARRAY_ITEMS: &[&[u8]] = &[
        b"source", b"cksums", b"md5sums", b"sha1sums", b"sha224sums",
        b"sha256sums", b"sha384sums", b"sha512sums", b"b2sums",
        b"depends", b"makedepends", b"checkdepends", b"optdepends",
        b"conflicts", b"provides", b"replaces"];
    const PACKAGE_ARCH_SPECIFIC_ARRAY_ITEMS: &[&[u8]] = &[
        b"checkdepends", b"depends", b"optdepends", b"provides", 
        b"conflicts", b"replaces"];
    let mut buffer = Vec::with_capacity(8192);
    buffer.extend_from_slice(include_bytes!(
        "src/script/10_source_config_and_start_loop.bash"));
    buffer_extend_multi_dump_plain(&mut buffer, 
        PKGBUILD_PLAIN_ITEMS, 1);
    buffer_extend_dump_array_license_workaround(&mut buffer, 1);
    buffer_extend_multi_dump_array(&mut buffer, 
        PKGBUILD_ARRAY_ITEMS, 1);
    buffer.extend_from_slice(include_bytes!(
        "src/script/20_pkgver_and_arch.bash"));
    buffer_extend_multi_dump_array(&mut buffer, 
        PKGBUILD_ARCH_SPECIFIC_ARRAY_ITEMS, 1);
    buffer.extend_from_slice(include_bytes!(
        "src/script/30_arch_end_any_init_other.bash"));
    buffer_extend_dump_arch_array(&mut buffer, 
        PKGBUILD_ARCH_SPECIFIC_ARRAY_ITEMS, 3);
    buffer.extend_from_slice(include_bytes!(
        "src/script/40_arch_end_other_package_start.bash"));
    buffer.extend_from_slice(include_bytes!(
        "src/script/50_pkg_until_plain.bash"));
    buffer_extend_multicase(&mut buffer, PACKAGE_PLAIN_ITEMS);
    buffer.extend_from_slice(include_bytes!(
        "src/script/60_pkg_until_array.bash"));
    for items in PACKAGE_ARRAY_ITEMS.iter() {
        buffer.push(b'|');
        buffer.extend_from_slice(items)
    }
    buffer.extend_from_slice(b")|(");
    buffer_extend_multicase(&mut buffer, 
        PACKAGE_ARCH_SPECIFIC_ARRAY_ITEMS);
    buffer.extend_from_slice(include_bytes!(
        "src/script/70_pkg_array_til_dump.bash"));
    buffer_extend_multi_dump_plain(&mut buffer, 
        PACKAGE_PLAIN_ITEMS, 2);
    buffer_extend_dump_array_license_workaround(&mut buffer, 2);
    buffer_extend_multi_dump_array(&mut buffer, 
        PACKAGE_ARRAY_ITEMS, 2);
    buffer_extend_indent(&mut buffer, 2);
    buffer.extend_from_slice(b"echo PACKAGEARCH\n");
    buffer_extend_indent(&mut buffer, 2);
    buffer.extend_from_slice(b"echo arch:any\n");
    buffer_extend_multi_dump_array(&mut buffer, 
        PACKAGE_ARCH_SPECIFIC_ARRAY_ITEMS, 2);
    buffer.extend_from_slice(include_bytes!(
        "src/script/80_pkg_arch_end_any_init_other.bash"));
    buffer_extend_dump_arch_array(&mut buffer, 
        PACKAGE_ARCH_SPECIFIC_ARRAY_ITEMS, 4);
    buffer.extend_from_slice(include_bytes!(
        "src/script/90_pkg_end_other.bash"));
    buffer_extend_indent(&mut buffer, 1);
    buffer.extend_from_slice(b"echo END\n)\ndone\n");
    file.write_all(&buffer).expect("Failed to write to script");
}