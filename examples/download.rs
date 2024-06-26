fn main() {
    let pkgbuild = pkgbuild::parse_one(
        std::env::args_os().nth(1)).unwrap();
    println!("Note: This does nothing to actually download the sources, but \
        just demonstrates how a program should use pkgbuild-rs's strongly \
        typed source to determine how to download them in Rust natively");
    for source_with_checksum in pkgbuild.sources_with_checksums() {
        let source = &source_with_checksum.source;
        println!("=> Downloading '{}' from '{}'...", source.name, source.url);
        match &source.protocol {
            pkgbuild::SourceProtocol::Unknown => 
                println!(" -> Impossible to download, unknown protocol"),
            pkgbuild::SourceProtocol::Local =>
                println!(" -> Skipped downloading for local file"),
            pkgbuild::SourceProtocol::File => 
                println!(" -> File cloning..."),
            pkgbuild::SourceProtocol::Ftp => 
                println!(" -> FTP downloading..."),
            pkgbuild::SourceProtocol::Http => 
                println!(" -> HTTP downloading..."),
            pkgbuild::SourceProtocol::Https => 
                println!(" -> HTTPS downloading..."),
            pkgbuild::SourceProtocol::Rsync => 
                println!(" -> rsync downloading..."),
            pkgbuild::SourceProtocol::Bzr { fragment } => 
                if let Some(fragment) = fragment {
                    println!(" -> Bzr cloning ({})...", fragment)
                } else {
                    println!(" -> Bzr cloning...")
                },
            pkgbuild::SourceProtocol::Fossil { fragment } => 
                if let Some(fragment) = fragment {
                    println!(" -> Fossil cloning ({})...", fragment)
                } else {
                    println!(" -> Fossil cloning...")
                },
            pkgbuild::SourceProtocol::Git { fragment, signed } => 
            {
                if let Some(fragment) = fragment {
                    let signed = if *signed {", signed"} else {""};
                    println!(" -> Git cloning ({}{})...", fragment, signed)
                } else if *signed {
                    println!(" -> Git cloning (signed)...")
                } else {
                    println!(" -> Git cloneing...")
                }
            },
            pkgbuild::SourceProtocol::Hg { fragment } => 
                if let Some(fragment) = fragment {
                    println!(" -> Hg cloning ({})...", fragment)
                } else {
                    println!(" -> Hg cloning...")
                },
            pkgbuild::SourceProtocol::Svn { fragment } =>
                if let Some(fragment) = fragment {
                    println!(" -> Svn cloning ({})...", fragment)
                } else {
                    println!(" -> Svn cloning...")
                },
        };
        if let Some(sha256sum) = source_with_checksum.sha256sum {
            print!(" -> Verifying sha256sum: ");
            for byte in sha256sum.iter() {
                print!("{:02x}", byte)
            }
            println!()
        } else {
            println!(" -> Skipped sha256sum check")
        }
    }


}