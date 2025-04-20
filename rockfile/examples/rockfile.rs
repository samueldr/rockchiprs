use std::{
    fs::File,
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Result};
use clap::Parser;
use rockfile::boot::{
    RkBootEntry, RkBootEntryBytes, RkBootHeader, RkBootHeaderBytes, RkBootHeaderEntry,
};

// Parameters for the Rockchip-flavoured CRC32 check
pub const CRC_32_RK: crc::Algorithm<u32> = crc::Algorithm {
    width: 32,
    poly: 0x04c10db7,
    init: 0x00000000,
    refin: false,
    refout: false,
    xorout: 0x00000000,
    check: 0x00000000,
    residue: 0x00000000,
};

fn parse_entry(header: RkBootHeaderEntry, name: &str, file: &mut File) -> Result<()> {
    for i in 0..header.count {
        let mut entry: RkBootEntryBytes = [0; 57];
        file.seek(SeekFrom::Start(
            header.offset as u64 + (header.size * i) as u64,
        ))?;
        file.read_exact(&mut entry)?;
        let entry = RkBootEntry::from_bytes(&entry);
        println!("== {} Entry  {} ==", name, i);
        println!("Name: {}", String::from_utf16(entry.name.as_slice())?);
        println!("Raw: {:?}", entry);

        let mut data = vec![0; entry.data_size as usize];
        file.seek(SeekFrom::Start(entry.data_offset as u64))?;
        file.read_exact(&mut data)?;

        let crc = crc::Crc::<u16>::new(&crc::CRC_16_IBM_3740);
        println!("Data CRC: {:x}", crc.checksum(&data));
    }

    Ok(())
}

fn parse_crc(file: &mut File) -> Result<()> {
    // CRC is the last four bytes
    file.seek(SeekFrom::End(-4))?;
    let mut file_crc_bytes = [0; 4];
    file.read_exact(&mut file_crc_bytes)?;

    let file_crc = u32::from_le_bytes(file_crc_bytes);

    // We need the length to re-compute the CRC.
    let file_length: usize = file.stream_position().unwrap() as usize;

    // Which is computed on everything beforehand
    file.seek(SeekFrom::Start(0))?;

    let mut data = vec![0; file_length - 4];
    file.read_exact(&mut data)?;

    let crc = crc::Crc::<u32>::new(&CRC_32_RK);
    let computed = crc.checksum(&data);

    // Print the information
    println!("    File CRC: 0x{:x}", file_crc);
    println!("Computed CRC: 0x{:x}", computed);
    if computed == file_crc {
        println!(" -> ok")
    } else {
        println!(" -> INVALID!")
    }

    Ok(())
}

fn parse_boot(path: &Path) -> Result<()> {
    let mut file = File::open(path)?;
    let mut header: RkBootHeaderBytes = [0; 102];
    file.read_exact(&mut header)?;
    let header =
        RkBootHeader::from_bytes(&header).ok_or_else(|| anyhow!("Failed to parse header"))?;

    println!("Raw Header: {:?}", header);
    println!(
        "chip: {:?} - {}",
        header.supported_chip,
        String::from_utf8_lossy(&header.supported_chip)
    );
    parse_entry(header.entry_471, "0x471", &mut file)?;
    parse_entry(header.entry_472, "0x472", &mut file)?;
    parse_entry(header.entry_loader, "loader", &mut file)?;
    parse_crc(&mut file)?;
    Ok(())
}

fn update_crc(path: &Path) -> Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    let mut header: RkBootHeaderBytes = [0; 102];
    file.read_exact(&mut header)?;

    // Nominally validate by parsing the header.
    // We don't want to clobber a random file's last four bytes.
    RkBootHeader::from_bytes(&header).ok_or_else(|| anyhow!("Failed to parse header. Is this a Rockchip Boot file?"))?;

    // The CRC is at the last four bytes
    file.seek(SeekFrom::End(-4))?;

    // We need the content length to re-compute the CRC, so save it.
    let content_length: usize = file.stream_position().unwrap() as usize;

    // Read the current CRC, we're going to show it to the user, and say whether we are updating it
    // or not.
    let mut file_crc_bytes = [0; 4];
    file.read_exact(&mut file_crc_bytes)?;
    let file_crc = u32::from_le_bytes(file_crc_bytes);

    // The CRC is computed on everything from the start up to those last four bytes.
    file.seek(SeekFrom::Start(0))?;

    let mut data = vec![0; content_length];
    file.read_exact(&mut data)?;

    let crc = crc::Crc::<u32>::new(&CRC_32_RK);
    let computed = crc.checksum(&data);

    // Print the information.
    println!("Original CRC: 0x{:x}", file_crc);
    println!("     New CRC: 0x{:x}", computed);

    // Maybe update...
    if computed != file_crc {
        println!("... updating!");
        file.seek(SeekFrom::End(-4))?;
        file.write(&computed.to_le_bytes())?;
    } else {
        println!("... already correct.");
    };

    Ok(())
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    /// Prints information about a Rockchip Boot File.
    BootFile { path: PathBuf },
    /// Updates the CRC of a Rockchip Boot File.
    UpdateCrc { path: PathBuf },
}

#[derive(clap::Parser)]
struct Opts {
    #[command(subcommand)]
    command: Commands,
}

fn main() -> Result<()> {
    let opt = Opts::parse();

    // Commands that don't talk a device
    match opt.command {
        Commands::BootFile { path } => parse_boot(&path),
        Commands::UpdateCrc { path } => update_crc(&path),
    }
}
