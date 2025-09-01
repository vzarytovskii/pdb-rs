//! Content handlers for different PDB data types
//!
//! This module provides specialized handlers for rendering different types of PDB content
//! in the TUI, separating data access logic from UI rendering logic.

use anyhow::Result;
use ms_pdb::codeview::IteratorWithRangesExt;
use ms_pdb::dbi::{DbiSourcesSubstream, ModuleInfo, DBI_STREAM_VERSION_V110, DBI_STREAM_VERSION_V50, DBI_STREAM_VERSION_V60, DBI_STREAM_VERSION_V70, DBI_STREAM_VERSION_VC41};
use ms_pdb::dbi::optional_dbg::OptionalDebugHeaderStream;
use ms_pdb::names::NameIndex;
use ms_pdb::pdbi::FeatureCode;
use ms_pdb::syms::SymIter;
use ms_pdb::{hash, Pdb, Stream, ReadAt};
use ratatui::widgets::{ListItem, Row};

use crate::dump::sym::DumpSymsContext;

use super::widgets::{helpers, MultiWidget};

/// Macro to create table rows more concisely
macro_rules! row {
    ($($cell:expr),* $(,)?) => {
        Row::new(vec![$($cell.to_string()),*])
    };
}


/// Trait for content that can be displayed in the TUI
pub trait ContentProvider {
    fn get_content(&self, pdb: &Pdb) -> Result<MultiWidget<'static>>;
    
    /// Get content with area dimensions for responsive layout
    fn get_content_with_area(&self, pdb: &Pdb, _area_width: u16, _area_height: u16) -> Result<MultiWidget<'static>> {
        self.get_content(pdb)
    }
}

/// Debug information content handler
pub struct DebugInfoHandler;

impl ContentProvider for DebugInfoHandler {
    fn get_content(&self, pdb: &Pdb) -> Result<MultiWidget<'static>> {
        let dbi_stream_header = pdb.read_dbi_stream_header()?;
        let dbi_stream = pdb.read_dbi_stream()?;
        let sources_substream = DbiSourcesSubstream::parse(dbi_stream.source_info())?;

        let mut file_name_offsets: Vec<u32> = sources_substream
            .file_name_offsets()
            .iter()
            .map(|x| x.get())
            .collect();
        file_name_offsets.sort_unstable();
        file_name_offsets.dedup();

        let is_mini_pdb = pdb.has_feature(FeatureCode::MINI_PDB);

        let pdb_file_format_version = dbi_stream_header.version.get();

        let pdb_file_format_version_string = match pdb_file_format_version {
            DBI_STREAM_VERSION_VC41 => "MSVC 4.1",
            DBI_STREAM_VERSION_V50 => "MSVC 5.0",
            DBI_STREAM_VERSION_V60 => "MSVC 6.0",
            DBI_STREAM_VERSION_V70 => "MSVC 7.0",
            DBI_STREAM_VERSION_V110 => "MSVC 11.0",
            _ => "Unknown",
        };

        let num_streams = pdb.num_streams();

        let container = match pdb.container() {
            ms_pdb::Container::Msf(_) => "MSF",
            ms_pdb::Container::Msfz(_) => "MSFZ",
        };

        let rows = vec![
            row!["Container Type", container],
            row![
                "PDB File Format Version",
                format!(
                    "{pdb_file_format_version_string} ({version}; 0x{version:08x})",
                    version = pdb_file_format_version
                )
            ],
            row![
                "Build Number",
                format!(
                    "{build} (0x{build:08x})",
                    build = dbi_stream_header.build_number.get()
                )
            ],
            row![
                "PDB File Age (amount of times file was changed)",
                format!("{age} (0x{age:08x})", age = dbi_stream_header.age.get())
            ],
            row![
                "Version of MSPDB.DLL",
                dbi_stream_header.pdb_dll_version.to_string()
            ],
            row![
                "PDB File Flags",
                format!("0x{flags:08x}", flags = dbi_stream_header.flags.get())
            ],
            row![
                "Is Mini PDB (/FASTLINK)",
                format!("{is_mini_pdb}", is_mini_pdb = is_mini_pdb)
            ],
            row![
                "Number of Streams",
                format!("{num_streams}", num_streams = num_streams)
            ],
            row![
                "Number of Modules",
                format!(
                    "{num_modules}",
                    num_modules = sources_substream.num_modules()
                )
            ],
            row![
                "Number of Sources (unique)",
                format!(
                    "{num_sources}",
                    num_sources = file_name_offsets.len()
                )
            ],
            row![
                "Number of Sources (not unique)",
                format!(
                    "{num_sources}",
                    num_sources = sources_substream.file_name_offsets().len()
                )
            ],
        ];

        let table = helpers::create_table(rows);
        Ok(MultiWidget::single(
            table,
            ratatui::layout::Constraint::Min(1),
        ))
    }
}

/// Modules content handler
pub struct ModulesHandler;

impl ContentProvider for ModulesHandler {
    fn get_content(&self, pdb: &Pdb) -> Result<MultiWidget<'static>> {
        let dbi_stream = pdb.read_dbi_stream()?;
        let modules_substream = dbi_stream.modules();
        let sources_substream = DbiSourcesSubstream::parse(dbi_stream.source_info())?;

        let mut module_infos: Vec<ModuleInfo> = Vec::new();
        module_infos.extend(modules_substream.iter());

        let num_modules = module_infos.len();

        let summary = helpers::create_paragraph(format!(
            "Modules ({num_modules} total)"
        ));

        let items = module_infos
            .iter()
            .enumerate()
            .map(|(module_index, module)| {
                let mut sources_acc = String::new();

                if let Ok(name_offsets) = sources_substream.name_offsets_for_module(module_index) {
                    for name_offset in name_offsets {
                        match sources_substream.get_source_file_name_at(name_offset.get()) {
                            Ok(name) => {
                                sources_acc.push_str(&format!("\n   [{:08x}] : {}", name_offset.get(), name));
                            }
                            Err(e) => {
                                sources_acc.push_str(&format!("\n   [{:08x}] : <error: {}>", name_offset.get(), e));
                            }
                        }
                    }
                }

                let display_text = if sources_acc.is_empty() {
                    format!(
                        "Module #{module_index}: {module_name}\n   Object: {obj_file}",
                        module_name = module.module_name(),
                        obj_file = module.obj_file()
                    )
                } else {
                    format!(
                        "Module #{module_index}: {module_name}\n   Object: {obj_file}\n   Sources:\n   {sources_acc}",
                        module_name = module.module_name(),
                        obj_file = module.obj_file()
                    )
                };

                // TODO: Don't display sources by default, there's an issue with rendering lists with bit list items.
                let display_text =
                    format!(
                        "Module #{module_index}: {module_name}\n╰─ Object: {obj_file}",
                        module_name = module.module_name(),
                        obj_file = module.obj_file()
                    );

                ListItem::new(display_text)
            })
            .collect::<Vec<_>>();

        let list = helpers::create_list(items);

        Ok(MultiWidget::new()
            .add(summary, ratatui::layout::Constraint::Length(3))
            .add(list, ratatui::layout::Constraint::Min(10)))
    }
}

/// Names content handler
pub struct NamesHandler;

impl ContentProvider for NamesHandler {
    fn get_content(&self, pdb: &Pdb) -> Result<MultiWidget<'static>> {
        let names_stream = pdb.names()?;
        let names_stream_index = pdb.named_stream_err(ms_pdb::names::NAMES_STREAM_NAME)?;

        let num_names = names_stream.num_strings;
        let num_hashes = names_stream.num_hashes;

        let summary = helpers::create_paragraph(format!("Stream index: {names_stream_index}\nNames: {num_names} total, {num_hashes} hashes."));

        let hashes = names_stream.hashes();
        let mut num_hashes_good: usize = 0;
        let mut num_hashes_bad: usize = 0;
        let mut num_hashes_unused: usize = 0;
        let mut probing_hash_base: u32 = 0;

        for (i, &ni) in hashes.iter().enumerate() {
            if ni.get() == 0 {
                num_hashes_unused += 1;
                probing_hash_base = i as u32 + 1;
                continue;
            }

            let s = names_stream.get_string(NameIndex(ni.get()))?;
            let computed_hash = hash::hash_mod_u32(s, names_stream.num_hashes as u32);
            let hash_is_good = computed_hash == i as u32
                || (computed_hash >= probing_hash_base && computed_hash < i as u32);
            if hash_is_good {
                num_hashes_good += 1;
            } else {
                num_hashes_bad += 1;
            }
        }

        let num_hashes_used = num_hashes_good + num_hashes_bad;
        let hash_state = if num_hashes_used != names_stream.num_strings {
            format!(
                "Error: Number of hashes used is {}, which is not equal to the total number of strings ({}).",
                num_hashes_used,
                names_stream.num_strings
            )
        } else {
            format!("Number of hashes used is equal to total number of strings (good).")

        };

        let hash_summary = helpers::create_paragraph(format!("Hash slots: {num_hashes_used} used\n   - {num_hashes_good} good\n   - {num_hashes_bad} bad\n   - {num_hashes_unused} unused.\n{hash_state}"));

        let items: Vec<ListItem> = names_stream
            .iter()
            .with_ranges()
            .map(|(range, name)| {
                ListItem::new(format!("[{:08x}] {name:?}", range.start))
            })
            .collect();

        let list = helpers::create_list(items);

        Ok(MultiWidget::new()
            .add(summary, ratatui::layout::Constraint::Percentage(5))
            .add(hash_summary, ratatui::layout::Constraint::Percentage(8))
            .add(list, ratatui::layout::Constraint::Min(10)))
    }
}

/// Globals content handler
pub struct GlobalsHandler;

impl ContentProvider for GlobalsHandler {
    fn get_content(&self, pdb: &Pdb) -> Result<MultiWidget<'static>> {
        // Try to read global symbol stream
        let gss = match pdb.gss() {
            Ok(gss) => gss,
            Err(e) => {
                let error_content = helpers::create_paragraph(format!(
                    "Global Symbol Stream not available\n\nError: {}\n\nThis PDB may not contain global symbols or the stream may be corrupted.", e
                ));
                return Ok(MultiWidget::single(
                    error_content,
                    ratatui::layout::Constraint::Min(1),
                ));
            }
        };

        let type_stream = pdb.read_type_stream()?;
        let ipi = pdb.read_ipi_stream()?;
        let symbol_records = &gss.stream_data;

        // Create iterator for symbol records
        let iter = SymIter::new(symbol_records).with_ranges();
        let mut context = DumpSymsContext::new(&type_stream, &ipi);
        context.show_type_index = true;

        let mut global_symbols = Vec::new();
        let mut num_symbols = 0;
        let stream_offset = 0;

        // Process each symbol record
        for (record_range, sym) in iter {
            let mut symbol_info = String::new();
            
            // Format the symbol using the existing dump_sym function
            if let Err(e) = crate::dump::sym::dump_sym(
                &mut symbol_info,
                &mut context,
                stream_offset + record_range.start as u32,
                sym.kind,
                sym.data,
            ) {
                symbol_info = format!("Error parsing symbol: {}", e);
            }

            // Clean up the output and create list item
            let display_text = symbol_info
                .lines()
                .next()
                .unwrap_or("Invalid symbol")
                .trim()
                .to_string();

            global_symbols.push(ListItem::new(format!(
                "[{:08x}] {}",
                record_range.start,
                display_text
            )));

            num_symbols += 1;
        }

        // Create summary information
        let summary = helpers::create_paragraph(format!(
            "Global Symbols ({} total)\nStream size: {} bytes",
            num_symbols,
            symbol_records.len()
        ));

        // Handle empty case
        if global_symbols.is_empty() {
            let empty_content = helpers::create_paragraph(
                "No global symbols found in this PDB file".to_string()
            );
            return Ok(MultiWidget::new()
                .add(summary, ratatui::layout::Constraint::Length(3))
                .add(empty_content, ratatui::layout::Constraint::Min(5)));
        }

        // Create the symbols list
        let list = helpers::create_list(global_symbols);

        Ok(MultiWidget::new()
            .add(summary, ratatui::layout::Constraint::Length(3))
            .add(list, ratatui::layout::Constraint::Min(10)))
    }
}

/// Types content handler
pub struct TypesHandler;

impl ContentProvider for TypesHandler {
    fn get_content(&self, pdb: &Pdb) -> Result<MultiWidget<'static>> {
        let _type_stream = pdb.read_type_stream()?;

        // TODO: Implement type browsing
        let content = helpers::create_paragraph(
            "Type information\n\nTODO: Implement type browser with hierarchical view".to_string(),
        );
        Ok(MultiWidget::single(
            content,
            ratatui::layout::Constraint::Min(1),
        ))
    }
}

/// Symbols content handler
pub struct SymbolsHandler;

impl ContentProvider for SymbolsHandler {
    fn get_content(&self, _pdb: &Pdb) -> Result<MultiWidget<'static>> {
        // TODO: Implement symbol browsing
        let content = helpers::create_paragraph(
            "Symbol information\n\nTODO: Implement symbol browser with search capabilities"
                .to_string(),
        );
        Ok(MultiWidget::single(
            content,
            ratatui::layout::Constraint::Min(1),
        ))
    }
}

pub struct StreamsHandler;

/// Stream usage information for TUI display
#[derive(Debug, Clone)]
enum StreamUsage {
    OldStreamDir, // 0
    PDB,          // 1
    TPI,          // 2
    DBI,          // 3
    IPI,          // 4
    ModuleInfo {
        module_name: String,
        obj_name: String,
    },
    Named {
        name: String,
    },
    GlobalSymbolStream,
    GlobalSymbolIndex,
    PublicSymbolStream,
    OptionalDebugHeader {
        which: usize,
        whichs: Option<OptionalDebugHeaderStream>,
    },
    TypeStreamHashStream {
        parent_stream: Stream,
    },
    TypeStreamAuxHashStream {
        parent_stream: Stream,
    },
}

impl ContentProvider for StreamsHandler {
    fn get_content(&self, pdb: &Pdb) -> Result<MultiWidget<'static>> {
        let num_streams = pdb.num_streams();

        // Initialize stream usage tracking
        let mut streams_usage: Vec<Option<StreamUsage>> =
            (0..num_streams as usize).map(|_| None).collect();

        streams_usage[0] = Some(StreamUsage::OldStreamDir);
        streams_usage[Stream::PDB.index()] = Some(StreamUsage::PDB);
        streams_usage[Stream::TPI.index()] = Some(StreamUsage::TPI);
        streams_usage[Stream::DBI.index()] = Some(StreamUsage::DBI);
        streams_usage[Stream::IPI.index()] = Some(StreamUsage::IPI);

        let mut add_stream_usage = |stream_opt: Option<u32>, usage: StreamUsage| {
            let Some(stream_index) = stream_opt else {
                return;
            };

            if let Some(slot) = streams_usage.get_mut(stream_index as usize) {
                *slot = Some(usage);
            }
        };

        // Read DBI information to classify more streams
        if let Ok(dbi_stream) = pdb.read_dbi_stream() {
            if let Ok(dbi_header) = dbi_stream.header() {
                add_stream_usage(
                    dbi_header.global_stream_index().ok(),
                    StreamUsage::GlobalSymbolIndex,
                );
                add_stream_usage(
                    dbi_header.sym_record_stream().ok(),
                    StreamUsage::GlobalSymbolStream,
                );
                add_stream_usage(
                    dbi_header.public_stream_index().ok(),
                    StreamUsage::PublicSymbolStream,
                );
            }

            // Add module streams
            for module in dbi_stream.modules().iter() {
                add_stream_usage(
                    module.stream(),
                    StreamUsage::ModuleInfo {
                        module_name: module.module_name().to_string(),
                        obj_name: module.obj_file().to_string(),
                    },
                );
            }

            // Add optional debug header streams
            if let Ok(optional_debug_header) = dbi_stream.optional_debug_header() {
                for (i, stream) in optional_debug_header.iter_streams() {
                    add_stream_usage(
                        Some(stream),
                        StreamUsage::OptionalDebugHeader {
                            which: i,
                            whichs: OptionalDebugHeaderStream::try_from(i).ok(),
                        },
                    );
                }
            }
        }

        // Add type stream hash streams
        if let Ok(tpi_header_info) = pdb.tpi_header() {
            if let Some(tpi_header) = tpi_header_info.header() {
                add_stream_usage(
                    tpi_header.hash_stream_index.get(),
                    StreamUsage::TypeStreamHashStream {
                        parent_stream: Stream::TPI,
                    },
                );
                add_stream_usage(
                    tpi_header.hash_aux_stream_index.get(),
                    StreamUsage::TypeStreamAuxHashStream {
                        parent_stream: Stream::TPI,
                    },
                );
            }
        }

        if let Ok(ipi_header_info) = pdb.ipi_header() {
            if let Some(ipi_header) = ipi_header_info.header() {
                add_stream_usage(
                    ipi_header.hash_stream_index.get(),
                    StreamUsage::TypeStreamHashStream {
                        parent_stream: Stream::IPI,
                    },
                );
                add_stream_usage(
                    ipi_header.hash_aux_stream_index.get(),
                    StreamUsage::TypeStreamAuxHashStream {
                        parent_stream: Stream::IPI,
                    },
                );
            }
        }

        // Add named streams
        let pdb_info = pdb.pdbi();
        for (name, stream) in pdb_info.named_streams().iter() {
            add_stream_usage(
                Some(*stream),
                StreamUsage::Named {
                    name: name.to_string(),
                },
            );
        }

        // Create summary
        let mut num_valid = 0;
        let mut num_invalid = 0;
        let mut num_unknown_usage = 0;

        for stream_index in 0..num_streams {
            if pdb.is_stream_valid(stream_index) {
                num_valid += 1;
                if streams_usage[stream_index as usize].is_none() {
                    num_unknown_usage += 1;
                }
            } else {
                num_invalid += 1;
            }
        }

        let summary = helpers::create_paragraph(format!(
            "PDB Streams ({num_streams} total)\n└─ {num_valid} valid, {num_invalid} invalid, {num_unknown_usage} unknown usage"
        ));

        // Create stream list
        let mut items = Vec::new();
        for (stream_index, usage_opt) in streams_usage.iter().enumerate() {
            let stream_index = stream_index as u32;
            let stream_size = pdb.stream_len(stream_index);
            let is_valid = pdb.is_stream_valid(stream_index);

            let status_icon = if is_valid { "✓" } else { "✗" };
            
            let usage_text = if let Some(usage) = usage_opt {
                format!("{:?}", usage)
            } else if is_valid {
                "UNKNOWN USAGE".to_string()
            } else {
                "nil".to_string()
            };

            let display_text = format!(
                "{} Stream #{:3} │ {:>10} bytes │ {}",
                status_icon, stream_index, stream_size, usage_text
            );

            items.push(ListItem::new(display_text));
        }

        let list = helpers::create_list(items);

        Ok(MultiWidget::new()
            .add(summary, ratatui::layout::Constraint::Length(3))
            .add(list, ratatui::layout::Constraint::Min(10)))
    }
}

/// Individual stream content handler with hex dump
pub struct SingleStreamHandler {
    pub stream_index: u32,
}

impl ContentProvider for SingleStreamHandler {
    fn get_content(&self, pdb: &Pdb) -> Result<MultiWidget<'static>> {
        self.get_content_with_area(pdb, 80, 24) // Default fallback dimensions
    }
    
    fn get_content_with_area(&self, pdb: &Pdb, area_width: u16, _area_height: u16) -> Result<MultiWidget<'static>> {
        let stream_size = pdb.stream_len(self.stream_index);
        
        if !pdb.is_stream_valid(self.stream_index) {
            let content = helpers::create_paragraph(format!(
                "Stream #{} is invalid (nil stream)", self.stream_index
            ));
            return Ok(MultiWidget::single(
                content,
                ratatui::layout::Constraint::Min(1),
            ));
        }

        // Determine stream usage type for better context
        let stream_usage = self.determine_stream_usage(pdb);
        
        // Create detailed stream info summary
        let summary_text = format!(
            "Stream #{} ({}) - {} bytes\n└─ Type: {}",
            self.stream_index,
            if stream_size > 1024 { 
                format!("{:.1} KB", stream_size as f64 / 1024.0)
            } else { 
                format!("{} bytes", stream_size) 
            },
            stream_size,
            stream_usage
        );
        let summary = helpers::create_paragraph(summary_text);

        // Read stream data for hex dump with improved sizing
        let max_bytes = if stream_size < 2048 { 
            stream_size as usize // Show full content for small streams
        } else { 
            2048 // Show first 2KB for larger streams
        };
        let mut stream_data = Vec::new();
        
        // Try to read the stream data
        if let Ok(stream) = pdb.read_stream(self.stream_index) {
            let bytes_to_read = std::cmp::min(stream_size as usize, max_bytes);
            stream_data.resize(bytes_to_read, 0);
            if let Ok(bytes_read) = stream.read_at(&mut stream_data, 0) {
                stream_data.truncate(bytes_read);
            }
        }

        let hex_dump = if stream_data.is_empty() {
            helpers::create_paragraph("No data could be read from this stream".to_string())
        } else {
            let lines_to_show = if stream_size < 512 { None } else { Some(48) }; // Show more lines for better context
            helpers::create_hex_dump_with_width(&stream_data, 0, lines_to_show, Some(area_width))
        };

        // Add navigation hint
        let hint = helpers::create_paragraph(
            "Navigation: ↑/↓ scroll | PgUp/PgDn page | Tab switch panels | q quit".to_string()
        );

        Ok(MultiWidget::new()
            .add(summary, ratatui::layout::Constraint::Length(4))
            .add(hex_dump, ratatui::layout::Constraint::Min(15))
            .add(hint, ratatui::layout::Constraint::Length(3)))
    }
}

impl SingleStreamHandler {
    fn determine_stream_usage(&self, pdb: &Pdb) -> String {
        // Classify common stream types for better user understanding
        match self.stream_index {
            0 => "Old Stream Directory".to_string(),
            1 => "PDB Info Stream".to_string(),
            2 => "Type Info (TPI)".to_string(),
            3 => "Debug Info (DBI)".to_string(),
            4 => "ID to Name Mapping (IPI)".to_string(),
            _ => {
                // Try to determine usage from DBI information
                if let Ok(dbi_stream) = pdb.read_dbi_stream() {
                    if let Ok(dbi_header) = dbi_stream.header() {
                        if let Ok(gsi) = dbi_header.global_stream_index() {
                            if gsi == self.stream_index {
                                return "Global Symbol Index".to_string();
                            }
                        }
                        if let Ok(gss) = dbi_header.sym_record_stream() {
                            if gss == self.stream_index {
                                return "Global Symbol Stream".to_string();
                            }
                        }
                        if let Ok(psi) = dbi_header.public_stream_index() {
                            if psi == self.stream_index {
                                return "Public Symbol Stream".to_string();
                            }
                        }
                    }
                    
                    // Check if it's a module stream
                    for (i, module) in dbi_stream.modules().iter().enumerate() {
                        if let Some(module_stream) = module.stream() {
                            if module_stream == self.stream_index {
                                return format!("Module #{} ({})", i, module.module_name());
                            }
                        }
                    }
                }
                
                // Check named streams
                let pdb_info = pdb.pdbi();
                for (name, &stream_index) in pdb_info.named_streams().iter() {
                    if stream_index == self.stream_index {
                        return format!("Named Stream ({})", name);
                    }
                }
                
                "Unknown Usage".to_string()
            }
        }
    }
}

pub fn get_content_handler(node_id: &super::navigation::TreeNodeId) -> Box<dyn ContentProvider> {
    match node_id {
        super::navigation::TreeNodeId::DebugInfo => Box::new(DebugInfoHandler),
        super::navigation::TreeNodeId::Modules => Box::new(ModulesHandler),
        super::navigation::TreeNodeId::Names => Box::new(NamesHandler),
        super::navigation::TreeNodeId::Globals => Box::new(GlobalsHandler),
        super::navigation::TreeNodeId::Types => Box::new(TypesHandler),
        super::navigation::TreeNodeId::Symbols => Box::new(SymbolsHandler),
        super::navigation::TreeNodeId::Streams => Box::new(StreamsHandler),
        super::navigation::TreeNodeId::Stream { index } => Box::new(SingleStreamHandler { 
            stream_index: *index 
        }),
        _ => Box::new(DefaultHandler),
    }
}

struct DefaultHandler;

impl ContentProvider for DefaultHandler {
    fn get_content(&self, _pdb: &Pdb) -> Result<MultiWidget<'static>> {
        let content = helpers::create_paragraph("No content available for this item".to_string());
        Ok(MultiWidget::single(
            content,
            ratatui::layout::Constraint::Min(1),
        ))
    }
}
