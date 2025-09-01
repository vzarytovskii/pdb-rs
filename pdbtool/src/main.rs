#![forbid(unused_must_use)]
#![feature(let_chains)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::manual_map)]
#![allow(clippy::single_match)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_late_init)]

use clap::Parser;

mod addsrc;
mod container;
mod copy;
mod counts;
mod dump;
mod dump_utils;
mod find;
mod glob_pdbs;
mod hexdump;
mod pdz;
mod save;
mod util;

#[derive(clap::Parser)]
struct CommandWithFlags {
    /// Reduce logging to just warnings and errors in `mspdb` and `pdbtool` modules.
    #[arg(long)]
    quiet: bool,

    /// Turn on debug output in all `mspdb` and `pdbtool` modules. Noisy!
    #[arg(long)]
    verbose: bool,

    /// Show timestamps in log messages
    #[arg(long)]
    timestamps: bool,

    /// Connect to Tracy (diagnostics tool). Requires that the `tracy` Cargo feature be enabled.
    #[arg(long)]
    tracy: bool,

    /// Launch interactive TUI mode for exploring PDB files
    #[arg(long)]
    interactive: bool,

    /// The PDB file to process (required for interactive mode)
    #[arg(required_if_eq("interactive", "true"))]
    pdb_file: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Adds source file contents to the PDB. The contents are embedded directly within the PDB.
    /// WinDbg and Visual Studio can both extract the source files.
    AddSrc(addsrc::AddSrcOptions),
    /// Copies a PDB file (or a PDZ) file to another PDB file. All stream contents are preserved
    /// exactly, byte-for-byte. The blocks within streams are laid out sequentially. The output
    /// file is always a PDB file, not a PDZ. The input can be either PDB or PDZ.
    Copy(copy::Options),
    /// Show information about the file container. This indicates whether the file is using the
    /// PDB (MSF) or PDBZ (MSFZ) container format. This also shows some container-specific
    /// information.
    Container(container::ContainerOptions),
    Test,
    Dump(dump::DumpOptions),
    Save(save::SaveStreamOptions),
    Find(find::FindOptions),
    FindName(find::FindNameOptions),
    Counts(counts::CountsOptions),
    /// Dumps part of a file (any file, not just a PDB) as a hex dump. If you want to dump a
    /// specific stream, then use the `dump <filename> hex` command instead.
    Hexdump(hexdump::HexdumpOptions),
    PdzEncode(pdz::encode::PdzEncodeOptions),
}

mod tui;

fn main() -> anyhow::Result<()> {
    let command_with_flags = CommandWithFlags::parse();
    configure_tracing(&command_with_flags);

    if command_with_flags.interactive {
        let pdb_file = command_with_flags.pdb_file.unwrap();
        return tui::run_tui(pdb_file);
    }

    match command_with_flags.command {
        Some(Command::AddSrc(args)) => addsrc::command(args)?,
        Some(Command::Dump(args)) => dump::dump_main(args)?,
        Some(Command::Test) => {}
        Some(Command::Copy(args)) => copy::copy_command(&args)?,
        Some(Command::Save(args)) => save::save_stream(&args)?,
        Some(Command::Find(args)) => find::find_command(&args)?,
        Some(Command::FindName(args)) => find::find_name_command(&args)?,
        Some(Command::Counts(args)) => counts::counts_command(args)?,
        Some(Command::Hexdump(args)) => hexdump::command(args)?,
        Some(Command::PdzEncode(args)) => pdz::encode::pdz_encode(args)?,
        Some(Command::Container(args)) => container::container_command(&args)?,
        None => {
            eprintln!("Error: No command specified. Use --help for usage information.");
            std::process::exit(1);
        }
    }

    Ok(())
}

fn configure_tracing(args: &CommandWithFlags) {
    use tracing_subscriber::filter::LevelFilter;

    if args.tracy {
        #[cfg(feature = "tracy")]
        {
            use tracing_subscriber::layer::SubscriberExt;

            let layer = tracing_tracy::TracyLayer::default();
            tracing::subscriber::set_global_default(tracing_subscriber::registry().with(layer))
                .expect("setup tracy layer");

            return;
        }

        #[cfg(not(feature = "tracy"))]
        {
            eprintln!(
                "Tracing is not enabled in the build configuration.\n\
                 You can enable it by using 'cargo run --features \"tracy\"'."
            );
        }
    }

    let builder = tracing_subscriber::fmt();

    let max_level = if args.quiet {
        LevelFilter::ERROR
    } else if args.verbose {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };

    builder.with_max_level(max_level).with_ansi(false).init();
}
